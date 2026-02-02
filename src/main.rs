mod api;
mod config;
mod error;

use chrono::{Datelike, Weekday};
use clap::{Parser, Subcommand};
use tracing::{error, info};

use crate::api::PerfectGymClient;
use crate::config::Config;
use crate::error::Result;

#[derive(Parser)]
#[command(name = "gym_sniper")]
#[command(about = "Automatically book gym classes at the perfect moment")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available classes
    List {
        /// Number of days to show (default: 7)
        #[arg(short, long, default_value = "7")]
        days: u32,
    },
    /// Book a specific class by ID
    Book {
        /// Class ID to book
        class_id: u64,
    },
    /// Run the scheduler to auto-book configured classes
    Schedule,
    /// Test login credentials
    Login,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("gym_sniper=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    let config = Config::load(&cli.config)?;
    let client = PerfectGymClient::new(&config);

    match cli.command {
        Commands::Login => {
            info!("Testing login...");
            client.login().await?;
            info!("Login successful!");
        }
        Commands::List { days } => {
            info!("Fetching classes for next {} days...", days);
            let client = client.login().await?;
            let classes = client.get_weekly_classes(days).await?;

            println!("\n{:<8} {:<30} {:<20} {:<20}", "ID", "Name", "Time", "Status");
            println!("{}", "-".repeat(80));

            for class in classes {
                println!(
                    "{:<8} {:<30} {:<20} {:<20}",
                    class.id,
                    truncate(&class.name, 28),
                    class.start_time.format("%a %d %b %H:%M"),
                    class.status
                );
            }
        }
        Commands::Book { class_id } => {
            info!("Booking class {}...", class_id);
            let client = client.login().await?;
            let result = client.book_class(class_id).await?;
            info!("Booked: {} at {}", result.name, result.start_time);
        }
        Commands::Schedule => {
            info!("Starting scheduler...");
            run_scheduler(config, client).await?;
        }
    }

    Ok(())
}

async fn run_scheduler(config: Config, client: PerfectGymClient) -> Result<()> {
    use chrono::{Duration, Local};
    use tokio::time::sleep;

    let client = client.login().await?;

    loop {
        let now = Local::now();
        info!("Checking for classes to book at {}", now.format("%Y-%m-%d %H:%M:%S"));

        // Get classes for the next 8 days (booking window is 7 days + 2 hours)
        let classes = client.get_weekly_classes(8).await?;

        for target in &config.targets {
            // Find matching classes
            for class in &classes {
                let class_time = class.start_time;
                let booking_opens = class_time - Duration::days(7) - Duration::hours(2);

                // Check if this class matches our target
                let day_matches = target.days.as_ref().map_or(true, |days| {
                    days.iter().any(|d| weekday_matches(d, class_time.weekday()))
                });

                let name_matches = class.name.to_lowercase().contains(&target.class_name.to_lowercase());
                let time_matches = target.time.as_ref().map_or(true, |t| {
                    class_time.format("%H:%M").to_string() == *t
                });

                if name_matches && day_matches && time_matches && class.status == "Bookable" {
                    // Check if booking window is open or about to open
                    let time_until_booking = booking_opens.signed_duration_since(now);

                    if time_until_booking.num_seconds() <= 0 {
                        info!("Booking window open for {} at {}", class.name, class_time);
                        match client.book_class(class.id).await {
                            Ok(result) => info!("Successfully booked: {}", result.name),
                            Err(e) => error!("Failed to book: {}", e),
                        }
                    } else if time_until_booking.num_minutes() <= 5 {
                        info!(
                            "Booking opens in {} seconds for {} at {}",
                            time_until_booking.num_seconds(),
                            class.name,
                            class_time
                        );
                        // Wait until booking opens
                        sleep(std::time::Duration::from_secs(
                            time_until_booking.num_seconds().max(0) as u64,
                        ))
                        .await;

                        // Try to book immediately
                        match client.book_class(class.id).await {
                            Ok(result) => info!("Successfully booked: {}", result.name),
                            Err(e) => error!("Failed to book: {}", e),
                        }
                    }
                }
            }
        }

        // Check every minute
        sleep(std::time::Duration::from_secs(60)).await;
    }
}

fn weekday_matches(day_str: &str, weekday: Weekday) -> bool {
    matches!(
        (day_str.to_lowercase().as_str(), weekday),
        ("monday" | "mon", Weekday::Mon)
            | ("tuesday" | "tue", Weekday::Tue)
            | ("wednesday" | "wed", Weekday::Wed)
            | ("thursday" | "thu", Weekday::Thu)
            | ("friday" | "fri", Weekday::Fri)
            | ("saturday" | "sat", Weekday::Sat)
            | ("sunday" | "sun", Weekday::Sun)
    )
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
