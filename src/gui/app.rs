use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;

use crate::api::{ClassInfo, MyBooking};
use crate::config::Config;
use crate::gui::async_bridge::{run_async_bridge, Command, Response};
use crate::gui::views::bookings::BookingsView;
use crate::gui::views::search::{SearchState, SearchView};
use crate::gui::views::snipe_queue::SnipeQueueView;
use crate::snipe_queue::SnipeEntry;

pub struct GymSniperApp {
    cmd_tx: Sender<Command>,
    resp_rx: Receiver<Response>,

    bookings: Vec<MyBooking>,
    snipe_queue: Vec<SnipeEntry>,
    search_results: Vec<ClassInfo>,
    search_state: SearchState,

    loading: bool,
    status_message: Option<(String, bool)>, // (message, is_error)
    message_timer: f32,
}

impl GymSniperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let (resp_tx, resp_rx) = channel();

        // Start the async bridge
        run_async_bridge(config, cmd_rx, resp_tx, cc.egui_ctx.clone());

        // Trigger initial data refresh
        let _ = cmd_tx.send(Command::RefreshBookings);
        let _ = cmd_tx.send(Command::RefreshSnipeQueue);

        Self {
            cmd_tx,
            resp_rx,
            bookings: Vec::new(),
            snipe_queue: Vec::new(),
            search_results: Vec::new(),
            search_state: SearchState {
                days_offset: 7,
                ..Default::default()
            },
            loading: false,
            status_message: None,
            message_timer: 0.0,
        }
    }

    fn process_responses(&mut self) {
        while let Ok(response) = self.resp_rx.try_recv() {
            match response {
                Response::BookingsLoaded(bookings) => {
                    self.bookings = bookings;
                }
                Response::SnipeQueueLoaded(queue) => {
                    self.snipe_queue = queue;
                }
                Response::SearchResults(results) => {
                    self.search_results = results;
                }
                Response::OperationSuccess(msg) => {
                    self.status_message = Some((msg, false));
                    self.message_timer = 5.0;
                }
                Response::OperationError(msg) => {
                    self.status_message = Some((msg, true));
                    self.message_timer = 8.0;
                }
                Response::Loading(loading) => {
                    self.loading = loading;
                }
            }
        }
    }
}

impl eframe::App for GymSniperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process any pending responses
        self.process_responses();

        // Update message timer
        if self.message_timer > 0.0 {
            self.message_timer -= ctx.input(|i| i.stable_dt);
            if self.message_timer <= 0.0 {
                self.status_message = None;
            }
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Status bar at the top
            if let Some((ref msg, is_error)) = self.status_message {
                let color = if is_error {
                    egui::Color32::from_rgb(220, 50, 50)
                } else {
                    egui::Color32::from_rgb(50, 180, 50)
                };

                egui::Frame::none()
                    .fill(color)
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.colored_label(egui::Color32::WHITE, msg);
                    });
                ui.add_space(8.0);
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Confirmed Bookings section
                ui.group(|ui| {
                    BookingsView::show(ui, &self.bookings, self.loading, &self.cmd_tx);
                });

                ui.add_space(16.0);

                // Snipe Queue section
                ui.group(|ui| {
                    SnipeQueueView::show(ui, &self.snipe_queue, self.loading, &self.cmd_tx);
                });

                ui.add_space(16.0);

                // Search section
                ui.group(|ui| {
                    SearchView::show(
                        ui,
                        &mut self.search_state,
                        &self.search_results,
                        self.loading,
                        &self.cmd_tx,
                    );
                });
            });
        });
    }
}
