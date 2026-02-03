mod api;
mod config;
mod email;
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
    /// Search classes by trainer name
    Trainer {
        /// Trainer name to search for (partial match, case-insensitive)
        name: String,
        /// Number of days to search (default: 28)
        #[arg(short, long, default_value = "28")]
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

            println!("\n{:<8} {:<30} {:<15} {:<20} {:<12}", "ID", "Name", "Trainer", "Time", "Status");
            println!("{}", "-".repeat(87));

            for class in classes {
                let trainer = class.trainer.as_deref().unwrap_or("-");
                println!(
                    "{:<8} {:<30} {:<15} {:<20} {:<12}",
                    class.id,
                    truncate(&class.name, 28),
                    truncate(trainer, 13),
                    class.start_time.format("%a %d %b %H:%M"),
                    class.status
                );
            }
        }
        Commands::Trainer { name, days } => {
            info!("Searching for trainer '{}' in next {} days...", name, days);
            let client = client.login().await?;
            let classes = client.get_weekly_classes(days).await?;

            let search = name.to_lowercase();
            let filtered: Vec<_> = classes
                .into_iter()
                .filter(|c| {
                    c.trainer
                        .as_ref()
                        .map(|t| t.to_lowercase().contains(&search))
                        .unwrap_or(false)
                })
                .collect();

            if filtered.is_empty() {
                println!("\nNo classes found for trainer matching '{}'", name);
            } else {
                println!("\n{:<8} {:<30} {:<15} {:<20} {:<12}", "ID", "Name", "Trainer", "Time", "Status");
                println!("{}", "-".repeat(87));

                for class in filtered {
                    let trainer = class.trainer.as_deref().unwrap_or("-");
                    println!(
                        "{:<8} {:<30} {:<15} {:<20} {:<12}",
                        class.id,
                        truncate(&class.name, 28),
                        truncate(trainer, 13),
                        class.start_time.format("%a %d %b %H:%M"),
                        class.status
                    );
                }
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
                println!("\n{:<8} {:<30} {:<15} {:<20} {:<12} {:<10}", "ID", "Name", "Trainer", "Time", "Status", "Waitlist");
                println!("{}", "-".repeat(97));

                for booking in bookings {
                    let waitlist = match booking.waitlist_position {
                        Some(pos) => format!("#{}", pos),
                        None => "-".to_string(),
                    };
                    let trainer = booking.trainer.as_deref().unwrap_or("-");
                    println!(
                        "{:<8} {:<30} {:<15} {:<20} {:<12} {:<10}",
                        booking.id,
                        truncate(&booking.name, 28),
                        truncate(trainer, 13),
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
    use rand::Rng;

    // Get initial class details
    let booking = client.get_class_details(class_id).await?;
    let class_time = booking.start_time;
    let estimated_booking_opens = class_time - Duration::days(7) - Duration::hours(2);

    info!(
        "Target: {} at {}",
        booking.name,
        class_time.format("%a %d %b %H:%M")
    );
    info!(
        "Estimated booking window: {}",
        estimated_booking_opens.format("%a %d %b %H:%M:%S")
    );
    info!("Current status: {}", booking.status);

    // If already bookable, try immediately
    if booking.status == "Bookable" {
        info!("Class is already bookable! Attempting to book...");
        return attempt_booking(config, class_id).await;
    }

    // If already booked or on waitlist, nothing to do
    if booking.status == "Booked" || booking.status == "Awaiting" {
        info!("Already booked or on waitlist for this class!");
        return Ok(());
    }

    info!("Polling for status change (currently: {})...", booking.status);
    let mut last_status = booking.status;
    let mut poll_count = 0;
    let mut client = client.clone();

    loop {
        poll_count += 1;
        let now = Local::now();
        let time_until_estimated = estimated_booking_opens.signed_duration_since(now);

        // Determine poll interval based on proximity to estimated booking window
        let poll_interval_secs = if time_until_estimated.num_minutes() > 30 {
            // More than 30 min away: poll every 60 seconds
            60
        } else if time_until_estimated.num_minutes() > 5 {
            // 5-30 min away: poll every 30 seconds
            30
        } else if time_until_estimated.num_minutes() > 1 {
            // 1-5 min away: poll every 10 seconds
            10
        } else {
            // Less than 1 min or past estimated time: poll every 2 seconds
            2
        };

        // Re-login periodically to keep token fresh (every ~30 minutes)
        // Also refresh 10 minutes before window opens to be ready
        let should_refresh = (poll_count % 30 == 0 && poll_interval_secs >= 60)
            || (time_until_estimated.num_seconds() > 590 && time_until_estimated.num_seconds() <= 600);
        if should_refresh {
            info!("Refreshing login token...");
            client = PerfectGymClient::new(config).login().await?;
        }

        // Check class status
        match client.get_class_details(class_id).await {
            Ok(details) => {
                if details.status != last_status {
                    info!(
                        "Status changed: {} -> {}",
                        last_status, details.status
                    );
                    last_status = details.status.clone();
                }

                match details.status.as_str() {
                    "Bookable" => {
                        info!("Class is now BOOKABLE! Starting booking attempts...");
                        return attempt_booking(config, class_id).await;
                    }
                    "Booked" | "Awaiting" => {
                        info!("Already booked or on waitlist!");
                        return Ok(());
                    }
                    "Unavailable" => {
                        // Class has started or ended
                        if class_time < now {
                            error!("Class has already started/ended without becoming bookable");
                            return Err(crate::error::GymSniperError::Api(
                                "Class is no longer available".to_string(),
                            ));
                        }
                    }
                    _ => {}
                }

                if poll_count % 10 == 1 || poll_interval_secs <= 10 {
                    info!(
                        "Poll #{}: status={}, est. window in {}, next poll in {}s",
                        poll_count,
                        details.status,
                        format_duration(time_until_estimated),
                        poll_interval_secs
                    );
                }
            }
            Err(e) => {
                error!("Poll #{}: Failed to get status: {}", poll_count, e);
                // Re-login on error in case token expired
                if format!("{}", e).contains("401") || format!("{}", e).contains("Unauthorized") {
                    info!("Token may have expired, refreshing...");
                    client = PerfectGymClient::new(config).login().await?;
                }
            }
        }

        // Add small random jitter to poll interval
        let mut rng = rand::thread_rng();
        let jitter_ms = rng.gen_range(0..1000);
        sleep(std::time::Duration::from_millis(
            (poll_interval_secs * 1000 + jitter_ms) as u64,
        ))
        .await;

        // Safety limit: stop after 24 hours of polling
        if poll_count > 24 * 60 * 60 / poll_interval_secs as usize {
            error!("Gave up after extended polling");
            return Err(crate::error::GymSniperError::Api(
                "Polling timeout".to_string(),
            ));
        }
    }
}

async fn attempt_booking(config: &Config, class_id: u64) -> Result<()> {
    use tokio::time::sleep;
    use rand::Rng;

    // Login token should already be fresh (refreshed 1 min before window)
    // but refresh again just in case this is called directly
    let client = PerfectGymClient::new(config).login().await?;

    // Get class details for email notifications
    let class_details = client.get_class_details(class_id).await.ok();
    let class_name = class_details.as_ref().map(|d| d.name.as_str()).unwrap_or("Unknown");
    let class_time = class_details.as_ref().map(|d| d.start_time.format("%a %d %b %H:%M").to_string()).unwrap_or_default();
    let class_trainer = class_details.as_ref().and_then(|d| d.trainer.as_deref());

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

                // Send success email
                if let Some(email_config) = &config.email {
                    let time_str = result.start_time.format("%a %d %b %H:%M").to_string();
                    email::send_booking_success(email_config, &result.name, &time_str, class_trainer).await;
                }

                return Ok(());
            }
            Err(e) => {
                let err_str = format!("{}", e);

                // Permanent failures - stop immediately
                if err_str.contains("DailyBookingLimitReached") {
                    error!("Daily booking limit reached - cannot book another class today");
                    if let Some(email_config) = &config.email {
                        email::send_booking_failure(
                            email_config,
                            class_name,
                            &class_time,
                            class_trainer,
                            "Daily booking limit reached - you already have a class booked on this day",
                        ).await;
                    }
                    return Err(crate::error::GymSniperError::Api("Daily booking limit reached".to_string()));
                }

                if err_str.contains("TooSoonToBook") {
                    if attempts % 10 == 1 {
                        info!("Attempt #{}: Window not open yet, retrying...", attempts);
                    }
                } else if err_str.contains("already") || err_str.contains("Already") {
                    info!("Already booked or on waitlist!");
                    return Ok(());
                } else if err_str.contains("Full") || err_str.contains("full") || err_str.contains("Awaitable") {
                    // Class is full - try to join waitlist then stop
                    info!("Class is full, attempting to join waitlist...");
                    // The API should add us to waitlist, stop after a few more tries
                    if attempts >= 5 {
                        info!("Joined waitlist (or waitlist full)");
                        return Ok(());
                    }
                } else {
                    error!("Attempt #{}: {}", attempts, e);
                    // Unknown error - might be permanent, stop after a few tries
                    if attempts >= 10 {
                        if let Some(email_config) = &config.email {
                            email::send_booking_failure(
                                email_config,
                                class_name,
                                &class_time,
                                class_trainer,
                                &err_str,
                            ).await;
                        }
                        return Err(e);
                    }
                }
            }
        }

        // Stop after ~10 minutes of trying (with random delays, ~1500 attempts)
        if attempts > 1500 {
            error!("Gave up after {} attempts", attempts);

            // Send failure email
            if let Some(email_config) = &config.email {
                email::send_booking_failure(
                    email_config,
                    class_name,
                    &class_time,
                    class_trainer,
                    "Max booking attempts reached",
                ).await;
            }

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
                            Ok(result) => {
                                info!("Successfully booked: {}", result.name);
                                if let Some(email_config) = &config.email {
                                    let time_str = result.start_time.format("%a %d %b %H:%M").to_string();
                                    email::send_booking_success(email_config, &result.name, &time_str, class.trainer.as_deref()).await;
                                }
                            }
                            Err(e) => {
                                error!("Failed to book: {}", e);
                                if let Some(email_config) = &config.email {
                                    let time_str = class_time.format("%a %d %b %H:%M").to_string();
                                    email::send_booking_failure(email_config, &class.name, &time_str, class.trainer.as_deref(), &format!("{}", e)).await;
                                }
                            }
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
                            Ok(result) => {
                                info!("Successfully booked: {}", result.name);
                                if let Some(email_config) = &config.email {
                                    let time_str = result.start_time.format("%a %d %b %H:%M").to_string();
                                    email::send_booking_success(email_config, &result.name, &time_str, class.trainer.as_deref()).await;
                                }
                            }
                            Err(e) => {
                                error!("Failed to book: {}", e);
                                if let Some(email_config) = &config.email {
                                    let time_str = class_time.format("%a %d %b %H:%M").to_string();
                                    email::send_booking_failure(email_config, &class.name, &time_str, class.trainer.as_deref(), &format!("{}", e)).await;
                                }
                            }
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
