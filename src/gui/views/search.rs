use eframe::egui::{self, Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::api::ClassInfo;
use crate::gui::async_bridge::Command;
use crate::util::truncate;

pub struct SearchView;

#[derive(Default)]
pub struct SearchState {
    pub days_offset: u32,
    pub time_filter: String,
    pub class_filter: String,
    pub trainer_filter: String,
}

impl SearchView {
    pub fn show(
        ui: &mut Ui,
        state: &mut SearchState,
        results: &[ClassInfo],
        loading: bool,
        cmd_tx: &std::sync::mpsc::Sender<Command>,
    ) {
        ui.heading("ADD FUTURE CLASS");
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Day:");

            // Create day options (Today+7 to Today+21)
            let day_options: Vec<(u32, String)> = (7..=21)
                .map(|offset| {
                    let date = chrono::Local::now() + chrono::Duration::days(offset as i64);
                    (offset, date.format("%a %d %b").to_string())
                })
                .collect();

            let current_label = day_options
                .iter()
                .find(|(o, _)| *o == state.days_offset)
                .map(|(_, l)| l.as_str())
                .unwrap_or("Select day");

            egui::ComboBox::from_id_salt("day_selector")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for (offset, label) in &day_options {
                        ui.selectable_value(&mut state.days_offset, *offset, label);
                    }
                });

            ui.label("Time:");
            ui.add(
                egui::TextEdit::singleline(&mut state.time_filter)
                    .hint_text("HH:MM")
                    .desired_width(60.0),
            );

            ui.label("Class:");
            ui.add(
                egui::TextEdit::singleline(&mut state.class_filter)
                    .hint_text("Name")
                    .desired_width(100.0),
            );

            ui.label("Trainer:");
            ui.add(
                egui::TextEdit::singleline(&mut state.trainer_filter)
                    .hint_text("Name")
                    .desired_width(80.0),
            );

            if ui
                .add_enabled(!loading, egui::Button::new("Search"))
                .clicked()
            {
                let _ = cmd_tx.send(Command::SearchClasses {
                    days_offset: state.days_offset,
                    time_filter: if state.time_filter.is_empty() {
                        None
                    } else {
                        Some(state.time_filter.clone())
                    },
                    class_filter: if state.class_filter.is_empty() {
                        None
                    } else {
                        Some(state.class_filter.clone())
                    },
                    trainer_filter: if state.trainer_filter.is_empty() {
                        None
                    } else {
                        Some(state.trainer_filter.clone())
                    },
                });
            }

            if loading {
                ui.spinner();
            }
        });

        ui.add_space(16.0);
        ui.heading("SEARCH RESULTS");
        ui.add_space(8.0);

        if results.is_empty() {
            ui.label("No results. Use the search form above to find classes.");
            return;
        }

        const MAX_ROWS: usize = 10;
        const HEADER_HEIGHT: f32 = 20.0;
        const ROW_HEIGHT: f32 = 25.0;

        let needs_scroll = results.len() > MAX_ROWS;

        // Use unique ID to prevent scroll conflicts with other tables
        ui.push_id("search_results_table", |ui| {
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto().at_least(60.0)) // ID
                .column(Column::remainder().at_least(70.0)) // Class
                .column(Column::auto().at_least(96.0)) // Trainer
                .column(Column::auto().at_least(144.0)) // Class Time
                .column(Column::auto().at_least(80.0)) // Status
                .column(Column::auto().at_least(60.0)); // Actions

            if needs_scroll {
                table = table
                    .min_scrolled_height(0.0)
                    .max_scroll_height(HEADER_HEIGHT + MAX_ROWS as f32 * ROW_HEIGHT);
            }

            table.header(HEADER_HEIGHT, |mut header| {
                header.col(|ui| {
                    ui.strong("ID");
                });
                header.col(|ui| {
                    ui.strong("Class");
                });
                header.col(|ui| {
                    ui.strong("Trainer");
                });
                header.col(|ui| {
                    ui.strong("Class Time");
                });
                header.col(|ui| {
                    ui.strong("Status");
                });
                header.col(|ui| {
                    ui.strong("Actions");
                });
            })
            .body(|mut body| {
                for class in results {
                    body.row(25.0, |mut row| {
                        row.col(|ui| {
                            ui.label(class.id.to_string());
                        });
                        row.col(|ui| {
                            ui.label(truncate(&class.name, 25));
                        });
                        row.col(|ui| {
                            ui.label(
                                class
                                    .trainer
                                    .as_ref()
                                    .map(|t| truncate(t, 12))
                                    .unwrap_or_else(|| "-".to_string()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(class.start_time.format("%a %d %b %H:%M").to_string());
                        });
                        row.col(|ui| {
                            let color = match class.status.as_str() {
                                "Bookable" => Color32::GREEN,
                                "Full" => Color32::RED,
                                "Booked" => Color32::LIGHT_BLUE,
                                _ => Color32::GRAY,
                            };
                            ui.label(RichText::new(&class.status).color(color));
                        });
                        row.col(|ui| {
                            if ui
                                .add_enabled(!loading, egui::Button::new("Add"))
                                .clicked()
                            {
                                let _ = cmd_tx.send(Command::AddToSnipeQueue(class.clone()));
                            }
                        });
                    });
                }
            });
        });
    }
}
