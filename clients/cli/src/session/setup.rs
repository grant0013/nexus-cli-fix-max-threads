//! Session setup and initialization

use crate::analytics::set_wallet_address_for_reporting;
use crate::config::Config;
use crate::environment::Environment;
use crate::events::Event;
use crate::orchestrator::OrchestratorClient;
use crate::runtime::start_authenticated_worker;
use ed25519_dalek::SigningKey;
use std::error::Error;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use std::time::Duration;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

/// Session data for both TUI and headless modes
#[derive(Debug)]
pub struct SessionData {
    /// Event receiver for worker events
    pub event_receiver: mpsc::Receiver<Event>,
    /// Join handles for worker tasks
    pub join_handles: Vec<JoinHandle<()>>,
    /// Shutdown sender to stop all workers
    pub shutdown_sender: broadcast::Sender<()>,
    /// Shutdown sender for max tasks completion
    pub max_tasks_shutdown_sender: broadcast::Sender<()>,
    /// Node ID
    pub node_id: u64,
    /// Orchestrator client
    pub orchestrator: OrchestratorClient,
    /// Number of workers (for display purposes)
    pub num_workers: usize,
}

/// Warn the user if their available memory seems insufficient for the task(s) at hand
pub async fn warn_memory_configuration(max_threads: Option<u32>) {
    if let Some(threads) = max_threads {
        // 1. Provide the refresh kind to `.with_memory()` as required by the new API.
        let mut sys = System::new_with_specifics(
            RefreshKind::default().with_memory(MemoryRefreshKind::everything()),
        );

        // 2. Now that the specifics are set, just call the simple `.refresh_memory()` method.
        sys.refresh_memory();

        let available_memory = sys.available_memory();
        let projected_requirement =
            threads as u64 * crate::consts::cli_consts::PROJECTED_MEMORY_REQUIREMENT;

        if projected_requirement >= available_memory {
            crate::print_cmd_warn!(
                "OOM Warning",
                "Projected memory usage for {} threads ({:.2} GB) may exceed available system memory ({:.2} GB). If proving fails, consider reducing --max-threads.",
                threads,
                projected_requirement as f64 / 1_073_741_824.0, // Bytes to GB
                available_memory as f64 / 1_073_741_824.0      // Bytes to GB
            );
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}

/// Sets up an authenticated worker session
///
/// This function handles all the common setup required for both TUI and headless modes:
/// 1. Creates signing key for the prover
/// 2. Sets up shutdown channel
/// 3. Starts authenticated worker
/// 4. Returns session data for mode-specific handling
///
/// # Arguments
/// * `config` - Resolved configuration with node_id and client_id
/// * `env` - Environment to connect to
/// * `max_threads` - Optional maximum number of threads for proving
/// * `max_difficulty` - Optional override for task difficulty
///
/// # Returns
/// * `Ok(SessionData)` - Successfully set up session
/// * `Err` - Session setup failed
pub async fn setup_session(
    config: Config,
    env: Environment,
    check_mem: bool,
    max_threads: Option<u32>,
    max_tasks: Option<u32>,
    max_difficulty: Option<crate::nexus_orchestrator::TaskDifficulty>,
) -> Result<SessionData, Box<dyn Error>> {
    // Add context to parsing errors for easier debugging.
    let node_id = config.node_id.parse::<u64>().map_err(|e| {
        format!("Failed to parse node_id '{}': {}", config.node_id, e)
    })?;
    let client_id = config.user_id;

    // Create a signing key for the prover
    let mut csprng = rand_core::OsRng;
    let signing_key: SigningKey = SigningKey::generate(&mut csprng);

    // Create orchestrator client
    let orchestrator_client = OrchestratorClient::new(env.clone());

    // Warn the user if the memory demands of their configuration is risky
    if check_mem {
        warn_memory_configuration(max_threads).await;
    }

    // Clamp the number of workers to available physical CPUs
    let num_workers: usize = max_threads
        .map(|n| n as usize) // If user set --max-threads, honor it
        .unwrap_or_else(|| num_cpus::get_physical()); // Otherwise, default to all physical cores

    // Create shutdown channel - only one shutdown signal needed
    let (shutdown_sender, _) = broadcast::channel(1);

    // Set wallet for reporting
    set_wallet_address_for_reporting(config.wallet_address.clone());

    // Start authenticated worker (only mode we support now)
    let (event_receiver, join_handles, max_tasks_shutdown_sender) = start_authenticated_worker(
        node_id,
        signing_key,
        orchestrator_client.clone(),
        shutdown_sender.subscribe(),
        env,
        client_id,
        max_tasks,
        max_difficulty,
        num_workers,
    )
    .await;

    Ok(SessionData {
        event_receiver,
        join_handles,
        shutdown_sender,
        max_tasks_shutdown_sender,
        node_id,
        orchestrator: orchestrator_client,
        num_workers,
    })
}