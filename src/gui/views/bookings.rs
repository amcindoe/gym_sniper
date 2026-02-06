use eframe::egui::{self, Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::api::MyBooking;
use crate::gui::async_bridge::Command;
use crate::util::truncate;

pub struct BookingsView;

impl BookingsView {
    pub fn show(
        ui: &mut Ui,
        bookings: &[MyBooking],
        loading: bool,
        cmd_tx: &std::sync::mpsc::Sender<Command>,
    ) {
        ui.horizontal(|ui| {
            ui.heading("CONFIRMED BOOKINGS");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!loading, egui::Button::new("Refresh"))
                    .clicked()
                {
                    let _ = cmd_tx.send(Command::RefreshBookings);
                }
                if loading {
                    ui.spinner();
                }
            });
        });

        ui.add_space(8.0);

        if bookings.is_empty() {
            ui.label("No confirmed bookings found.");
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
            .column(Column::auto().at_least(120.0)) // Time
            .column(Column::auto().at_least(80.0)) // Status
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
                    ui.strong("Time");
                });
                header.col(|ui| {
                    ui.strong("Status");
                });
                header.col(|ui| {
                    ui.strong("Actions");
                });
            })
            .body(|mut body| {
                for booking in bookings {
                    body.row(25.0, |mut row| {
                        row.col(|ui| {
                            ui.label(booking.id.to_string());
                        });
                        row.col(|ui| {
                            ui.label(truncate(&booking.name, 25));
                        });
                        row.col(|ui| {
                            ui.label(
                                booking
                                    .trainer
                                    .as_ref()
                                    .map(|t| truncate(t, 12))
                                    .unwrap_or_else(|| "-".to_string()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(booking.start_time.format("%a %d %b %H:%M").to_string());
                        });
                        row.col(|ui| {
                            let (status_text, color): (String, Color32) = match booking.status.as_str() {
                                "Booked" => ("Booked".to_string(), Color32::GREEN),
                                "Awaiting" => {
                                    let pos = booking
                                        .waitlist_position
                                        .map(|p| format!("#{}", p))
                                        .unwrap_or_else(|| "Waitlist".to_string());
                                    (pos, Color32::YELLOW)
                                }
                                _ => (booking.status.clone(), Color32::GRAY),
                            };
                            ui.label(RichText::new(status_text).color(color));
                        });
                        row.col(|ui| {
                            if ui
                                .add_enabled(!loading, egui::Button::new("Cancel"))
                                .clicked()
                            {
                                let _ = cmd_tx.send(Command::CancelBooking(booking.id));
                            }
                        });
                    });
                }
            });
    }
}
