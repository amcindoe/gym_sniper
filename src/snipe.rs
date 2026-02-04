use chrono::{Duration, Local};
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::api::PerfectGymClient;
use crate::config::Config;
use crate::email;
use crate::error::Result;
use crate::snipe_queue::SnipeQueue;
use crate::util::format_duration;

/// Snipe a class - wait for booking window and book immediately
pub async fn snipe_class(config: &Config, client: &PerfectGymClient, class_id: u64) -> Result<()> {
    // Get initial class details
    let booking = client.get_class_details(class_id).await?;
    let class_time = booking.start_time;
    let booking_window_opens = class_time - Duration::days(7) - Duration::hours(2);

    info!(
        "Target: {} at {}",
        booking.name,
        class_time.format("%a %d %b %H:%M")
    );
    info!(
        "Booking window opens: {}",
        booking_window_opens.format("%a %d %b %H:%M:%S")
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

    let now = Local::now();
    let time_until_window = booking_window_opens.signed_duration_since(now);

    // If more than 1 minute until window, sleep until 1 minute before
    if time_until_window.num_minutes() > 1 {
        let wake_time = booking_window_opens - Duration::minutes(1);
        let sleep_duration = wake_time.signed_duration_since(now);

        info!(
            "Booking window in {}. Sleeping until {} (1 min before window)...",
            format_duration(time_until_window),
            wake_time.format("%a %d %b %H:%M:%S")
        );

        // Sleep in chunks to show progress
        let total_sleep_secs = sleep_duration.num_seconds().max(0) as u64;
        let mut slept_secs = 0u64;

        while slept_secs < total_sleep_secs {
            let remaining = total_sleep_secs - slept_secs;
            let chunk = remaining.min(3600); // Sleep max 1 hour at a time
            sleep(std::time::Duration::from_secs(chunk)).await;
            slept_secs += chunk;

            if remaining > 3600 {
                let hours_left = (remaining - chunk) / 3600;
                let mins_left = ((remaining - chunk) % 3600) / 60;
                info!("Still waiting... {}h {}m until snipe starts", hours_left, mins_left);
            }
        }
    }

    // Refresh token 1 minute before window
    info!("Refreshing login token...");
    let _client = PerfectGymClient::new(config).login().await?;
    info!("Token refreshed.");

    // Sleep until exactly when window opens
    let now = Local::now();
    let time_until_window = booking_window_opens.signed_duration_since(now);
    if time_until_window.num_milliseconds() > 0 {
        info!("Waiting {}ms until booking window opens...", time_until_window.num_milliseconds());
        sleep(std::time::Duration::from_millis(time_until_window.num_milliseconds() as u64)).await;
    }

    info!("Booking window open - starting booking attempts NOW!");
    attempt_booking(config, class_id).await
}

/// Attempt to book a class with retries
pub async fn attempt_booking(config: &Config, class_id: u64) -> Result<()> {
    // Login token should already be fresh from snipe_class
    // but refresh if this is called directly (e.g., from book command)
    let client = PerfectGymClient::new(config).login().await?;

    // Get class details for email notifications
    let class_details = client.get_class_details(class_id).await.ok();
    let class_name = class_details.as_ref().map(|d| d.name.as_str()).unwrap_or("Unknown");
    let class_time = class_details.as_ref().map(|d| d.start_time.format("%a %d %b %H:%M").to_string()).unwrap_or_default();
    let class_trainer = class_details.as_ref().and_then(|d| d.trainer.as_deref());

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 10;

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
                    info!("Attempt #{}: Window not open yet, retrying...", attempts);
                } else if err_str.contains("already") || err_str.contains("Already") {
                    info!("Already booked or on waitlist!");
                    return Ok(());
                } else if err_str.contains("Full") || err_str.contains("full") || err_str.contains("Awaitable") {
                    // Class is full - try to join waitlist
                    info!("Attempt #{}: Class is full, attempting to join waitlist...", attempts);
                } else {
                    error!("Attempt #{}: {}", attempts, e);
                }
            }
        }

        // Stop after max attempts
        if attempts >= MAX_ATTEMPTS {
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

        // Fixed 200ms delay between attempts
        sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Run the snipe daemon - continuously monitors and executes queued snipes
pub async fn run_snipe_daemon(config: &Config) -> Result<()> {
    info!("Snipe daemon started. Monitoring snipe queue...");

    loop {
        // Clean up old entries
        let mut queue = SnipeQueue::load()?;
        queue.cleanup_old_entries()?;

        // Get pending snipes
        let pending = queue.pending_snipes();

        if pending.is_empty() {
            info!("No pending snipes. Checking again in 60 seconds...");
            sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }

        // Find the next snipe (earliest booking window)
        let next_snipe = pending[0];
        let now = Local::now();
        let time_until_window = next_snipe.booking_window.signed_duration_since(now);

        info!(
            "Next snipe: {} at {} (window opens in {})",
            next_snipe.class_name,
            next_snipe.class_time.format("%a %d %b %H:%M"),
            format_duration(time_until_window)
        );

        // If window is more than 5 minutes away, sleep and check again
        if time_until_window.num_minutes() > 5 {
            let sleep_duration = if time_until_window.num_minutes() > 60 {
                // More than 1 hour away - check every 30 minutes
                std::time::Duration::from_secs(30 * 60)
            } else if time_until_window.num_minutes() > 30 {
                // 30-60 min away - check every 10 minutes
                std::time::Duration::from_secs(10 * 60)
            } else {
                // 5-30 min away - check every minute
                std::time::Duration::from_secs(60)
            };

            info!("Sleeping for {} seconds...", sleep_duration.as_secs());
            sleep(sleep_duration).await;
            continue;
        }

        // Time to snipe! Execute it
        let class_id = next_snipe.class_id;
        let class_name = next_snipe.class_name.clone();

        info!("Executing snipe for {} (class ID {})...", class_name, class_id);

        // Create fresh client for the snipe
        let client = match PerfectGymClient::new(config).login().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to login for snipe: {}", e);
                let mut queue = SnipeQueue::load()?;
                queue.mark_failed(class_id, &format!("Login failed: {}", e))?;
                continue;
            }
        };

        // Execute the snipe
        match snipe_class(config, &client, class_id).await {
            Ok(()) => {
                info!("Snipe successful for {}", class_name);
                let mut queue = SnipeQueue::load()?;
                queue.mark_completed(class_id)?;
            }
            Err(e) => {
                let err_str = format!("{}", e);
                if err_str.contains("DailyBookingLimitReached") {
                    warn!("Daily booking limit reached for {}", class_name);
                } else {
                    error!("Snipe failed for {}: {}", class_name, e);
                }
                let mut queue = SnipeQueue::load()?;
                queue.mark_failed(class_id, &err_str)?;
            }
        }

        // Brief pause before checking for next snipe
        sleep(std::time::Duration::from_secs(5)).await;
    }
}
