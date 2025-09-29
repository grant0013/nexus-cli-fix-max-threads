//! Single authenticated worker that orchestrates fetch→prove→submit using a pipelined approach.

use super::core::{EventSender, WorkerConfig};
use super::fetcher::TaskFetcher;
use super::prover::TaskProver;
use super::submitter::ProofSubmitter;
use crate::events::{Event, ProverState};
use crate::orchestrator::OrchestratorClient;
use crate::task::Task;

use ed25519_dalek::SigningKey;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

/// Single authenticated worker that handles the complete task lifecycle.
/// This implementation uses a pipelined approach to overlap network I/O with CPU-bound proving.
pub struct AuthenticatedWorker {
    fetcher: TaskFetcher,
    prover: TaskProver,
    submitter: ProofSubmitter,
    event_sender: EventSender,
    max_tasks: Option<u32>,
    tasks_completed: u32,
    shutdown_sender: broadcast::Sender<()>,
}

impl AuthenticatedWorker {
    pub fn new(
        node_id: u64,
        signing_key: SigningKey,
        orchestrator: OrchestratorClient,
        config: WorkerConfig,
        event_sender: mpsc::Sender<Event>,
        max_tasks: Option<u32>,
        shutdown_sender: broadcast::Sender<()>,
    ) -> Self {
        let event_sender_helper = EventSender::new(event_sender);

        // Create the 3 specialized components
        let fetcher = TaskFetcher::new(
            node_id,
            signing_key.verifying_key(),
            Box::new(orchestrator.clone()),
            event_sender_helper.clone(),
            &config,
        );
        let prover = TaskProver::new(event_sender_helper.clone(), config.clone());
        let submitter = ProofSubmitter::new(
            signing_key,
            Box::new(orchestrator),
            event_sender_helper.clone(),
            &config,
        );

        Self {
            fetcher,
            prover,
            submitter,
            event_sender: event_sender_helper,
            max_tasks,
            tasks_completed: 0,
            shutdown_sender,
        }
    }

    /// Start the worker's main event loop.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Vec<JoinHandle<()>> {
        let mut join_handles = Vec::new();

        self.event_sender
            .send_event(Event::state_change(
                ProverState::Waiting,
                "Ready to fetch tasks".to_string(),
            ))
            .await;

        // The main work loop is now self-contained within run_pipelined_cycle.
        let worker_handle = tokio::spawn(async move {
            tokio::select! {
                _ = shutdown.recv() => { /* Shutdown signal received, exit */ },
                _ = self.run_pipelined_cycle() => { /* Work cycle completed or failed, exit */ },
            }
        });
        join_handles.push(worker_handle);

        join_handles
    }

    /// Runs a continuous, pipelined work cycle.
    /// It fetches the next task while proving the current one.
    async fn run_pipelined_cycle(&mut self) {
        // Step 1: "Prime the pump" by fetching the first task.
        let mut current_task_to_prove = match self.fetcher.fetch_task().await {
            Ok(task) => task,
            Err(_) => {
                // If we can't get the first task, wait and exit. The main loop will restart.
                tokio::time::sleep(Duration::from_secs(5)).await;
                return;
            }
        };

        // The main pipeline loop
        loop {
            let start_time = Instant::now();

            // Step 2: Concurrently prove the current task AND fetch the next one.
            self.event_sender
                .send_event(Event::state_change(
                    ProverState::Proving,
                    format!("Step 2 of 4: Proving task {}", current_task_to_prove.task_id),
                ))
                .await;

            let (proof_result, next_task_result) = tokio::join!(
                self.prover.prove_task(&current_task_to_prove),
                self.fetcher.fetch_task()
            );

            // Step 3: Handle the results, starting with the proof.
            let proof = match proof_result {
                Ok(p) => p,
                Err(_) => {
                    self.event_sender
                        .send_event(Event::state_change(
                            ProverState::Waiting,
                            "Proof generation failed, fetching next task".to_string(),
                        ))
                        .await;
                    // Proving failed, but we may have a next task ready.
                    // Discard the failed task and cycle with the next one.
                    match next_task_result {
                        Ok(next_task) => {
                            current_task_to_prove = next_task;
                            continue; // Go to next loop iteration
                        }
                        Err(_) => {
                            // Both proving and fetching failed. Wait and exit.
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            return;
                        }
                    }
                }
            };

            // Step 4: Submit the successful proof.
            if self.submitter.submit_proof(&current_task_to_prove, &proof).await.is_ok() {
                // On successful submission, update tracking and check for completion.
                if self.handle_successful_submission(start_time, &current_task_to_prove).await {
                    return; // Max tasks reached, exit loop.
                }
            }
            // If submission fails, error is logged by submitter. Continue to next task.

            // Step 5: Prepare for the next cycle.
            match next_task_result {
                Ok(next_task) => {
                    current_task_to_prove = next_task;
                }
                Err(_) => {
                    // Proving succeeded, but we couldn't get a next task.
                    // Wait and exit; let the system restart the process.
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    return;
                }
            }
        }
    }

    /// Handles logic for a successful submission to keep the main loop clean.
    /// Returns true if the worker should shut down.
    async fn handle_successful_submission(&mut self, start_time: Instant, task: &Task) -> bool {
        self.tasks_completed += 1;
        let duration_secs = start_time.elapsed().as_secs();
        self.fetcher.update_success_tracking(duration_secs);

        self.event_sender
            .send_event(Event::state_change(
                ProverState::Waiting,
                format!(
                    "{} completed, Task size: {}, Duration: {}s, Difficulty: {}",
                    task.task_id,
                    task.public_inputs_list.len(),
                    self.fetcher.last_success_duration_secs.unwrap_or(0),
                    self.fetcher
                        .last_success_difficulty
                        .map(|d| d.as_str_name())
                        .unwrap_or("Unknown")
                ),
            ))
            .await;

        if let Some(max) = self.max_tasks {
            if self.tasks_completed >= max {
                self.event_sender
                    .send_event(Event::state_change(
                        ProverState::Waiting,
                        format!("Completed {} tasks, shutting down", self.tasks_completed),
                    ))
                    .await;
                // Give a moment for the message to be processed before shutting down.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = self.shutdown_sender.send(());
                return true; // Signal to exit
            }
        }
        false // Do not exit
    }
}


