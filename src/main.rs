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
    /// Show your booked and waitlisted classes
    Bookings,
    /// Snipe a class - wait for booking window and book immediately
    Snipe {
        /// Class ID to snipe
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
        Commands::Bookings => {
            info!("Fetching your bookings...");
            let client = client.login().await?;
            let bookings = client.get_my_bookings().await?;

            if bookings.is_empty() {
                println!("\nNo current bookings found.");
            } else {
                println!("\n{:<8} {:<30} {:<20} {:<12} {:<10}", "ID", "Name", "Time", "Status", "Waitlist");
                println!("{}", "-".repeat(82));

                for booking in bookings {
                    let waitlist = match booking.waitlist_position {
                        Some(pos) => format!("#{}", pos),
                        None => "-".to_string(),
                    };
                    println!(
                        "{:<8} {:<30} {:<20} {:<12} {:<10}",
                        booking.id,
                        truncate(&booking.name, 28),
                        booking.start_time.format("%a %d %b %H:%M"),
                        booking.status,
                        waitlist
                    );
                }
            }
        }
        Commands::Snipe { class_id } => {
            info!("Sniping class {}...", class_id);
            let client = client.login().await?;
            snipe_class(&config, &client, class_id).await?;
        }
        Commands::Schedule => {
            info!("Starting scheduler...");
            run_scheduler(config, client).await?;
        }
    }

    Ok(())
}

async fn snipe_class(config: &Config, client: &api::PerfectGymClient, class_id: u64) -> Result<()> {
    use chrono::{Duration, Local};
    use tokio::time::sleep;

    // Get class details to find when booking opens
    let booking = client.get_class_details(class_id).await?;
    let class_time = booking.start_time;
    let booking_opens = class_time - Duration::days(7) - Duration::hours(2);

    info!(
        "Target: {} at {}",
        booking.name,
        class_time.format("%a %d %b %H:%M")
    );
    info!(
        "Booking window opens: {}",
        booking_opens.format("%a %d %b %H:%M:%S")
    );

    // Wait until 1 minute before booking window opens
    loop {
        let now = Local::now();
        let wait_until = booking_opens - Duration::minutes(1);
        let time_to_wait = wait_until.signed_duration_since(now);

        if time_to_wait.num_seconds() <= 0 {
            break;
        }

        info!(
            "Waiting {} until snipe starts (1 min before window)...",
            format_duration(time_to_wait)
        );

        // Sleep in chunks so we can show progress
        let sleep_secs = time_to_wait.num_seconds().min(60) as u64;
        sleep(std::time::Duration::from_secs(sleep_secs)).await;
    }

    // Re-login to get fresh token (old one may have expired during wait)
    info!("Refreshing login token...");
    let client = PerfectGymClient::new(config).login().await?;
    info!("Login refreshed, starting snipe attempts...");

    // Try with random delays to appear more human-like
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut attempts = 0;

    loop {
        attempts += 1;

        match client.book_class(class_id).await {
            Ok(result) => {
                info!(
                    "SUCCESS! Booked {} at {} (attempt #{})",
                    result.name,
                    result.start_time.format("%a %d %b %H:%M"),
                    attempts
                );
                return Ok(());
            }
            Err(e) => {
                let err_str = format!("{}", e);
                if err_str.contains("TooSoonToBook") {
                    if attempts % 10 == 1 {
                        info!("Attempt #{}: Window not open yet, retrying...", attempts);
                    }
                } else if err_str.contains("already") || err_str.contains("Already") {
                    info!("Already booked or on waitlist!");
                    return Ok(());
                } else {
                    error!("Attempt #{}: {}", attempts, e);
                }
            }
        }

        // Stop after ~10 minutes of trying (with random delays, ~1500 attempts)
        if attempts > 1500 {
            error!("Gave up after {} attempts", attempts);
            return Err(crate::error::GymSniperError::Api("Max attempts reached".to_string()));
        }

        // Random delay between 200-500ms to appear more human-like
        let delay_ms = rng.gen_range(200..500);
        sleep(std::time::Duration::from_millis(delay_ms)).await;
    }
}

fn format_duration(d: chrono::Duration) -> String {
    let total_secs = d.num_seconds();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
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
