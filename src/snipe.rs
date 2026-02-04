use chrono::{Duration, Local};
use tokio::time::sleep;
use tracing::{error, info};

use crate::api::PerfectGymClient;
use crate::config::Config;
use crate::email;
use crate::error::Result;
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

    // If more than 5 minutes until window, sleep until 5 minutes before
    if time_until_window.num_minutes() > 5 {
        let wake_time = booking_window_opens - Duration::minutes(5);
        let sleep_duration = wake_time.signed_duration_since(now);

        info!(
            "Booking window in {}. Sleeping until {} (5 min before window)...",
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

    // Refresh token 5 minutes before window
    info!("Refreshing login token...");
    let client = PerfectGymClient::new(config).login().await?;
    info!("Token refreshed.");

    // Poll 1: ~5 minutes before
    let now = Local::now();
    let time_until_window = booking_window_opens.signed_duration_since(now);
    info!("Poll 1/3: {} until window, status check...", format_duration(time_until_window));

    if let Ok(details) = client.get_class_details(class_id).await {
        info!("Status: {}", details.status);
        if details.status == "Bookable" {
            info!("Already bookable! Attempting to book...");
            return attempt_booking(config, class_id).await;
        }
    }

    // Sleep until 1 minute before
    let now = Local::now();
    let one_min_before = booking_window_opens - Duration::minutes(1);
    let sleep_until_1min = one_min_before.signed_duration_since(now);
    if sleep_until_1min.num_seconds() > 0 {
        sleep(std::time::Duration::from_secs(sleep_until_1min.num_seconds() as u64)).await;
    }

    // Poll 2: ~1 minute before
    let now = Local::now();
    let time_until_window = booking_window_opens.signed_duration_since(now);
    info!("Poll 2/3: {} until window, status check...", format_duration(time_until_window));

    if let Ok(details) = client.get_class_details(class_id).await {
        info!("Status: {}", details.status);
        if details.status == "Bookable" {
            info!("Already bookable! Attempting to book...");
            return attempt_booking(config, class_id).await;
        }
    }

    // Sleep until 10 seconds before
    let now = Local::now();
    let ten_sec_before = booking_window_opens - Duration::seconds(10);
    let sleep_until_10sec = ten_sec_before.signed_duration_since(now);
    if sleep_until_10sec.num_seconds() > 0 {
        sleep(std::time::Duration::from_secs(sleep_until_10sec.num_seconds() as u64)).await;
    }

    // Poll 3: ~10 seconds before
    let now = Local::now();
    let time_until_window = booking_window_opens.signed_duration_since(now);
    info!("Poll 3/3: {} until window, status check...", format_duration(time_until_window));

    if let Ok(details) = client.get_class_details(class_id).await {
        info!("Status: {}", details.status);
        if details.status == "Bookable" {
            info!("Already bookable! Attempting to book...");
            return attempt_booking(config, class_id).await;
        }
    }

    // Sleep until 0.5 seconds before window opens
    let now = Local::now();
    let half_sec_before = booking_window_opens - Duration::milliseconds(500);
    let sleep_until_start = half_sec_before.signed_duration_since(now);
    if sleep_until_start.num_milliseconds() > 0 {
        info!("Waiting {}ms until booking attempts start...", sleep_until_start.num_milliseconds());
        sleep(std::time::Duration::from_millis(sleep_until_start.num_milliseconds() as u64)).await;
    }

    info!("Starting booking attempts NOW!");
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
    const MAX_ATTEMPTS: u32 = 15;

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
