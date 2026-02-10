use eframe::egui;
use eframe::egui::IconData;

use gym_sniper::config::Config;
use gym_sniper::gui::app::GymSniperApp;

fn load_icon() -> IconData {
    let size = 64u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;

            // Dark background circle
            if dist <= 30.0 {
                rgba[idx] = 45;     // R
                rgba[idx + 1] = 45; // G
                rgba[idx + 2] = 45; // B
                rgba[idx + 3] = 255;

                // Red target rings
                if (dist - 22.0).abs() < 1.5 || (dist - 14.0).abs() < 1.5 {
                    rgba[idx] = 231;
                    rgba[idx + 1] = 76;
                    rgba[idx + 2] = 60;
                }

                // Red bullseye
                if dist <= 6.0 {
                    rgba[idx] = 231;
                    rgba[idx + 1] = 76;
                    rgba[idx + 2] = 60;
                }

                // Crosshair lines
                let on_vertical = dx.abs() < 1.2 && (dist < 24.0) && !(dist >= 6.0 && dist <= 10.0);
                let on_horizontal = dy.abs() < 1.2 && (dist < 24.0) && !(dist >= 6.0 && dist <= 10.0);
                if on_vertical || on_horizontal {
                    rgba[idx] = 231;
                    rgba[idx + 1] = 76;
                    rgba[idx + 2] = 60;
                }
            }
        }
    }

    IconData {
        rgba,
        width: size,
        height: size,
    }
}

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
            .with_min_inner_size([600.0, 400.0])
            .with_app_id("gym-sniper")
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "The Laboratory - Classes",
        options,
        Box::new(|cc| Ok(Box::new(GymSniperApp::new(cc, config)))),
    )
}
