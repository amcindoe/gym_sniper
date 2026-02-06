use eframe::egui;

use gym_sniper::config::Config;
use gym_sniper::gui::app::GymSniperApp;

fn main() -> eframe::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("gym_sniper=info".parse().unwrap()),
        )
        .init();

    // Load config
    let config = Config::load("config.toml").expect("Failed to load config.toml");

    // Run the GUI
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("The Laboratory - Classes")
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "The Laboratory - Classes",
        options,
        Box::new(|cc| Ok(Box::new(GymSniperApp::new(cc, config)))),
    )
}
