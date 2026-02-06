use eframe::egui::{self, Ui};
use egui_extras::{Column, TableBuilder};

use crate::gui::async_bridge::Command;
use crate::snipe_queue::SnipeEntry;
use crate::util::truncate;

pub struct SnipeQueueView;

impl SnipeQueueView {
    pub fn show(
        ui: &mut Ui,
        snipes: &[SnipeEntry],
        loading: bool,
        cmd_tx: &std::sync::mpsc::Sender<Command>,
    ) {
        ui.horizontal(|ui| {
            ui.heading("FUTURE BOOKINGS (Snipe Queue)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!loading, egui::Button::new("Refresh"))
                    .clicked()
                {
                    let _ = cmd_tx.send(Command::RefreshSnipeQueue);
                }
                if loading {
                    ui.spinner();
                }
            });
        });

        ui.add_space(8.0);

        if snipes.is_empty() {
            ui.label("No classes in snipe queue.");
            return;
        }

        let available_height = ui.available_height().min(200.0);

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(60.0)) // ID
            .column(Column::remainder().at_least(120.0)) // Name
            .column(Column::auto().at_least(80.0)) // Trainer
            .column(Column::auto().at_least(120.0)) // Class Time
            .column(Column::auto().at_least(120.0)) // Window Opens
            .column(Column::auto().at_least(60.0)) // Actions
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("ID");
                });
                header.col(|ui| {
                    ui.strong("Name");
                });
                header.col(|ui| {
                    ui.strong("Trainer");
                });
                header.col(|ui| {
                    ui.strong("Class Time");
                });
                header.col(|ui| {
                    ui.strong("Window Opens");
                });
                header.col(|ui| {
                    ui.strong("Actions");
                });
            })
            .body(|mut body| {
                for snipe in snipes {
                    body.row(25.0, |mut row| {
                        row.col(|ui| {
                            ui.label(snipe.class_id.to_string());
                        });
                        row.col(|ui| {
                            ui.label(truncate(&snipe.class_name, 25));
                        });
                        row.col(|ui| {
                            ui.label(
                                snipe
                                    .trainer
                                    .as_ref()
                                    .map(|t| truncate(t, 12))
                                    .unwrap_or_else(|| "-".to_string()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(snipe.class_time.format("%a %d %b %H:%M").to_string());
                        });
                        row.col(|ui| {
                            ui.label(snipe.booking_window.format("%a %d %b %H:%M").to_string());
                        });
                        row.col(|ui| {
                            if ui
                                .add_enabled(!loading, egui::Button::new("Remove"))
                                .clicked()
                            {
                                let _ = cmd_tx.send(Command::RemoveFromSnipeQueue(snipe.class_id));
                            }
                        });
                    });
                }
            });
    }
}
