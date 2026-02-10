use clap::{Parser, Subcommand};
use tracing::{error, info};

use gym_sniper::api::PerfectGymClient;
use gym_sniper::config::Config;
use gym_sniper::error::Result;
use gym_sniper::scheduler;
use gym_sniper::snipe;
use gym_sniper::snipe_queue::{SnipeEntry, SnipeQueue, SnipeStatus};
use gym_sniper::util::{booking_window, truncate};

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
    /// List classes not yet bookable (booking window not open)
    Upcoming {
        /// Number of days to show (default: 7, max: 21)
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
    /// Snipe a class - wait for booking window and book immediately (single class)
    Snipe {
        /// Class ID to snipe
        class_id: u64,
    },
    /// Add a class to the snipe queue
    SnipeAdd {
        /// Class ID to add
        class_id: u64,
    },
    /// Remove a class from the snipe queue
    SnipeRemove {
        /// Class ID to remove
        class_id: u64,
    },
    /// List all queued snipes
    Snipes,
    /// Run the snipe daemon to automatically snipe all queued classes
    SnipeDaemon,
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
            client.login().await?;
            let classes = client.get_weekly_classes(days).await?;

            println!("\n{:<8} {:<25} {:<15} {:<20} {:<12}", "ID", "Class", "Trainer", "Class Time", "Status");
            println!("{}", "-".repeat(87));

            for class in classes {
                let trainer = class.trainer.as_deref().unwrap_or("-");
                println!(
                    "{:<8} {:<25} {:<15} {:<20} {:<12}",
                    class.id,
                    truncate(&class.name, 23),
                    truncate(trainer, 13),
                    class.start_time.format("%a %d %b %H:%M"),
                    class.status
                );
            }
        }
        Commands::Trainer { name, days } => {
            info!("Searching for trainer '{}' in next {} days...", name, days);
            client.login().await?;
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
                println!("\n{:<8} {:<25} {:<15} {:<20} {:<12}", "ID", "Class", "Trainer", "Class Time", "Status");
                println!("{}", "-".repeat(87));

                for class in filtered {
                    let trainer = class.trainer.as_deref().unwrap_or("-");
                    println!(
                        "{:<8} {:<25} {:<15} {:<20} {:<12}",
                        class.id,
                        truncate(&class.name, 23),
                        truncate(trainer, 13),
                        class.start_time.format("%a %d %b %H:%M"),
                        class.status
                    );
                }
            }
        }
        Commands::Upcoming { days } => {
            let days = days.min(21); // Cap at 21 days
            info!("Fetching upcoming classes (not yet bookable) for next {} days...", days);
            client.login().await?;

            // Need to fetch 7 days ahead of requested range since booking window is 7d+2h before class
            let fetch_days = days + 8;
            let classes = client.get_weekly_classes(fetch_days).await?;

            let now = chrono::Local::now();

            // Filter to classes where booking window hasn't opened yet
            let filtered: Vec<_> = classes
                .into_iter()
                .filter(|c| {
                    let window_opens = c.start_time - booking_window();
                    window_opens > now
                })
                .collect();

            if filtered.is_empty() {
                println!("\nNo upcoming unbookable classes found.");
            } else {
                println!("\n{:<8} {:<25} {:<15} {:<20} {:<20}", "ID", "Class", "Trainer", "Class Time", "Window Opens");
                println!("{}", "-".repeat(95));

                for class in filtered {
                    let trainer = class.trainer.as_deref().unwrap_or("-");
                    let window_opens = class.start_time - booking_window();
                    println!(
                        "{:<8} {:<25} {:<15} {:<20} {:<20}",
                        class.id,
                        truncate(&class.name, 23),
                        truncate(trainer, 13),
                        class.start_time.format("%a %d %b %H:%M"),
                        window_opens.format("%a %d %b %H:%M")
                    );
                }
            }
        }
        Commands::Book { class_id } => {
            info!("Booking class {}...", class_id);
            client.login().await?;
            let result = client.book_class(class_id).await?;
            info!("Booked: {} at {}", result.name, result.start_time);
        }
        Commands::Bookings => {
            info!("Fetching your bookings...");
            client.login().await?;
            let bookings = client.get_my_bookings().await?;

            if bookings.is_empty() {
                println!("\nNo current bookings found.");
            } else {
                println!("\n{:<8} {:<25} {:<15} {:<20} {:<12} {:<10}", "ID", "Class", "Trainer", "Class Time", "Status", "Waitlist");
                println!("{}", "-".repeat(97));

                for booking in bookings {
                    let waitlist = match booking.waitlist_position {
                        Some(pos) => format!("#{}", pos),
                        None => "-".to_string(),
                    };
                    let trainer = booking.trainer.as_deref().unwrap_or("-");
                    println!(
                        "{:<8} {:<25} {:<15} {:<20} {:<12} {:<10}",
                        booking.id,
                        truncate(&booking.name, 23),
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
            client.login().await?;
            snipe::snipe_class(&config, &client, class_id).await?;
        }
        Commands::SnipeAdd { class_id } => {
            info!("Adding class {} to snipe queue...", class_id);
            client.login().await?;

            // Get class details
            let details = client.get_class_details(class_id).await?;
            let bw = details.start_time - booking_window();

            let entry = SnipeEntry {
                class_id,
                class_name: details.name.clone(),
                class_time: details.start_time,
                booking_window: bw,
                trainer: details.trainer.clone(),
                added_at: chrono::Local::now(),
                status: SnipeStatus::Pending,
                error_message: None,
            };

            let mut queue = SnipeQueue::load()?;
            queue.add(entry)?;

            info!(
                "Added to snipe queue: {} at {} (window opens {})",
                details.name,
                details.start_time.format("%a %d %b %H:%M"),
                bw.format("%a %d %b %H:%M")
            );
        }
        Commands::SnipeRemove { class_id } => {
            let mut queue = SnipeQueue::load()?;
            if queue.remove(class_id)? {
                info!("Removed class {} from snipe queue", class_id);
            } else {
                error!("Class {} not found in snipe queue", class_id);
            }
        }
        Commands::Snipes => {
            let queue = SnipeQueue::load()?;
            let pending = queue.pending_snipes();

            if pending.is_empty() {
                println!("\nNo pending snipes in queue.");
            } else {
                println!("\n{:<8} {:<25} {:<12} {:<18} {:<18}", "ID", "Class", "Trainer", "Class Time", "Window Opens");
                println!("{}", "-".repeat(83));

                for snipe in pending {
                    let trainer = snipe.trainer.as_deref().unwrap_or("-");
                    println!(
                        "{:<8} {:<25} {:<12} {:<18} {:<18}",
                        snipe.class_id,
                        truncate(&snipe.class_name, 23),
                        truncate(trainer, 10),
                        snipe.class_time.format("%a %d %b %H:%M"),
                        snipe.booking_window.format("%a %d %b %H:%M")
                    );
                }
            }

            // Also show recent completed/failed
            let non_pending: Vec<_> = queue.snipes.iter()
                .filter(|s| s.status != SnipeStatus::Pending)
                .collect();

            if !non_pending.is_empty() {
                println!("\nRecent completed/failed:");
                println!("{:<8} {:<25} {:<18} {:<10}", "ID", "Class", "Class Time", "Status");
                println!("{}", "-".repeat(63));

                for snipe in non_pending {
                    let status = match snipe.status {
                        SnipeStatus::Completed => "Completed",
                        SnipeStatus::Failed => "Failed",
                        SnipeStatus::Pending => "Pending",
                    };
                    println!(
                        "{:<8} {:<25} {:<18} {:<10}",
                        snipe.class_id,
                        truncate(&snipe.class_name, 23),
                        snipe.class_time.format("%a %d %b %H:%M"),
                        status
                    );
                }
            }
        }
        Commands::SnipeDaemon => {
            info!("Starting snipe daemon...");
            snipe::run_snipe_daemon(&config).await?;
        }
        Commands::Schedule => {
            info!("Starting scheduler...");
            scheduler::run_scheduler(config, client).await?;
        }
    }

    Ok(())
}
