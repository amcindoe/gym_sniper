use std::future::Future;
use std::sync::mpsc::{Receiver, Sender};

use eframe::egui;
use tokio::runtime::Runtime;

use crate::api::{ClassInfo, MyBooking, PerfectGymClient};
use crate::config::Config;
use crate::snipe_queue::{SnipeEntry, SnipeQueue, SnipeStatus};
use crate::util::booking_window;

/// Commands sent from GUI to async thread
#[derive(Debug)]
pub enum Command {
    RefreshBookings,
    RefreshSnipeQueue,
    SearchClasses {
        days_offset: u32,
        time_filter: Option<String>,
        class_filter: Option<String>,
        trainer_filter: Option<String>,
    },
    AddToSnipeQueue(ClassInfo),
    RemoveFromSnipeQueue(u64),
    CancelBooking(u64),
}

/// Responses sent from async thread to GUI
#[derive(Debug)]
pub enum Response {
    BookingsLoaded(Vec<MyBooking>),
    SnipeQueueLoaded(Vec<SnipeEntry>),
    SearchResults(Vec<ClassInfo>),
    OperationSuccess(String),
    OperationError(String),
    Loading(bool),
}

/// Manages API client with automatic re-authentication on token expiration
struct ClientManager {
    config: Config,
    client: Option<PerfectGymClient>,
}

impl ClientManager {
    fn new(config: Config) -> Self {
        Self {
            config,
            client: None,
        }
    }

    /// Get a valid client, logging in if necessary
    async fn get_client(&mut self) -> Result<&PerfectGymClient, String> {
        if self.client.is_none() {
            self.login().await?;
        }
        Ok(self.client.as_ref().unwrap())
    }

    /// Force a fresh login
    async fn login(&mut self) -> Result<(), String> {
        let client = PerfectGymClient::new(&self.config);
        client.login()
            .await
            .map_err(|e| format!("Login failed: {}", e))?;
        self.client = Some(client);
        Ok(())
    }

    /// Invalidate the current client (call after auth errors)
    fn invalidate(&mut self) {
        self.client = None;
    }

    /// Execute an API call with automatic auth-retry on failure.
    /// Clones the client so the async block can own it without lifetime issues.
    async fn with_retry<T, F, Fut>(&mut self, f: F) -> Result<T, String>
    where
        F: Fn(PerfectGymClient) -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        let client = self.get_client().await?.clone();
        match f(client).await {
            Ok(val) => Ok(val),
            Err(e) if is_auth_error(&e) => {
                self.invalidate();
                let client = self.get_client().await?.clone();
                f(client).await
            }
            Err(e) => Err(e),
        }
    }
}

/// Check if an error is an authentication error
fn is_auth_error(error: &str) -> bool {
    error.contains("401")
        || error.contains("Unauthorized")
        || error.contains("Not logged in")
        || error.contains("token")
}

/// Runs the async bridge in a background thread
pub fn run_async_bridge(
    config: Config,
    cmd_rx: Receiver<Command>,
    resp_tx: Sender<Response>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let rt = Runtime::new().expect("Failed to create tokio runtime");

        rt.block_on(async {
            let mut manager = ClientManager::new(config);

            // Initial login
            if let Err(e) = manager.login().await {
                let _ = resp_tx.send(Response::OperationError(e));
                ctx.request_repaint();
            }

            loop {
                match cmd_rx.recv() {
                    Ok(cmd) => {
                        let _ = resp_tx.send(Response::Loading(true));
                        ctx.request_repaint();

                        match cmd {
                            Command::RefreshBookings => {
                                match manager.with_retry(|c| async move {
                                    c.get_my_bookings().await.map_err(|e| e.to_string())
                                }).await {
                                    Ok(bookings) => {
                                        let _ = resp_tx.send(Response::BookingsLoaded(bookings));
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::OperationError(format!(
                                            "Failed to load bookings: {}", e
                                        )));
                                    }
                                }
                            }
                            Command::RefreshSnipeQueue => {
                                match SnipeQueue::load() {
                                    Ok(queue) => {
                                        let mut pending: Vec<_> = queue
                                            .snipes
                                            .into_iter()
                                            .filter(|s| s.status == SnipeStatus::Pending)
                                            .collect();
                                        pending.sort_by_key(|s| s.class_time);
                                        let _ = resp_tx.send(Response::SnipeQueueLoaded(pending));
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::OperationError(format!(
                                            "Failed to load snipe queue: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Command::SearchClasses {
                                days_offset,
                                time_filter,
                                class_filter,
                                trainer_filter,
                            } => {
                                let fetch_days = days_offset + 7;

                                let classes = manager.with_retry(|c| async move {
                                    c.get_weekly_classes(fetch_days).await.map_err(|e| e.to_string())
                                }).await;

                                if let Ok(classes) = classes {
                                    let now = chrono::Local::now();
                                    let target_date =
                                        (now + chrono::Duration::days(days_offset as i64))
                                            .date_naive();

                                    let filtered: Vec<_> = classes
                                        .into_iter()
                                        .filter(|c| {
                                            if c.start_time.date_naive() != target_date {
                                                return false;
                                            }
                                            if let Some(ref time) = time_filter {
                                                if !time.is_empty() {
                                                    let class_time =
                                                        c.start_time.format("%H:%M").to_string();
                                                    if !class_time.starts_with(time) {
                                                        return false;
                                                    }
                                                }
                                            }
                                            if let Some(ref class_name) = class_filter {
                                                if !class_name.is_empty()
                                                    && !c.name.to_lowercase().contains(&class_name.to_lowercase())
                                                {
                                                    return false;
                                                }
                                            }
                                            if let Some(ref trainer) = trainer_filter {
                                                if !trainer.is_empty() {
                                                    if let Some(ref t) = c.trainer {
                                                        if !t.to_lowercase().contains(&trainer.to_lowercase()) {
                                                            return false;
                                                        }
                                                    } else {
                                                        return false;
                                                    }
                                                }
                                            }
                                            true
                                        })
                                        .collect();

                                    let _ = resp_tx.send(Response::SearchResults(filtered));
                                } else if let Err(e) = classes {
                                    let _ = resp_tx.send(Response::OperationError(format!(
                                        "Search failed: {}", e
                                    )));
                                }
                            }
                            Command::AddToSnipeQueue(class_info) => {
                                let bw = class_info.start_time - booking_window();

                                let entry = SnipeEntry {
                                    class_id: class_info.id,
                                    class_name: class_info.name.clone(),
                                    class_time: class_info.start_time,
                                    booking_window: bw,
                                    trainer: class_info.trainer.clone(),
                                    added_at: chrono::Local::now(),
                                    status: SnipeStatus::Pending,
                                    error_message: None,
                                };

                                match SnipeQueue::load() {
                                    Ok(mut queue) => match queue.add(entry) {
                                        Ok(()) => {
                                            let _ = resp_tx.send(Response::OperationSuccess(
                                                format!("Added {} to snipe queue", class_info.name),
                                            ));
                                            let mut pending: Vec<_> = queue
                                                .snipes
                                                .into_iter()
                                                .filter(|s| s.status == SnipeStatus::Pending)
                                                .collect();
                                            pending.sort_by_key(|s| s.class_time);
                                            let _ = resp_tx.send(Response::SnipeQueueLoaded(pending));
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::OperationError(
                                                format!("Failed to add to queue: {}", e),
                                            ));
                                        }
                                    },
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::OperationError(format!(
                                            "Failed to load queue: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Command::RemoveFromSnipeQueue(class_id) => {
                                match SnipeQueue::load() {
                                    Ok(mut queue) => match queue.remove(class_id) {
                                        Ok(true) => {
                                            let _ = resp_tx.send(Response::OperationSuccess(
                                                format!("Removed class {} from queue", class_id),
                                            ));
                                            let mut pending: Vec<_> = queue
                                                .snipes
                                                .into_iter()
                                                .filter(|s| s.status == SnipeStatus::Pending)
                                                .collect();
                                            pending.sort_by_key(|s| s.class_time);
                                            let _ = resp_tx.send(Response::SnipeQueueLoaded(pending));
                                        }
                                        Ok(false) => {
                                            let _ = resp_tx.send(Response::OperationError(
                                                format!("Class {} not found in queue", class_id),
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = resp_tx.send(Response::OperationError(
                                                format!("Failed to remove: {}", e),
                                            ));
                                        }
                                    },
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::OperationError(format!(
                                            "Failed to load queue: {}",
                                            e
                                        )));
                                    }
                                }
                            }
                            Command::CancelBooking(class_id) => {
                                match manager.with_retry(|c| async move {
                                    c.cancel_booking(class_id).await.map_err(|e| e.to_string())?;
                                    c.get_my_bookings().await.map_err(|e| e.to_string())
                                }).await {
                                    Ok(bookings) => {
                                        let _ = resp_tx.send(Response::OperationSuccess(
                                            format!("Cancelled booking for class {}", class_id),
                                        ));
                                        let _ = resp_tx.send(Response::BookingsLoaded(bookings));
                                    }
                                    Err(e) => {
                                        let _ = resp_tx.send(Response::OperationError(format!(
                                            "Failed to cancel booking: {}", e
                                        )));
                                    }
                                }
                            }
                        }

                        let _ = resp_tx.send(Response::Loading(false));
                        ctx.request_repaint();
                    }
                    Err(_) => {
                        // Channel closed, exit
                        break;
                    }
                }
            }
        });
    });
}
