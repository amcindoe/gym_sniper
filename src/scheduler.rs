use chrono::{Datelike, Duration, Local};
use tokio::time::sleep;
use tracing::{error, info};

use crate::api::PerfectGymClient;
use crate::config::Config;
use crate::email;
use crate::error::Result;
use crate::util::weekday_matches;

/// Run the scheduler to auto-book configured classes
pub async fn run_scheduler(config: Config, client: PerfectGymClient) -> Result<()> {
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
