use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::{error, info};

use crate::config::EmailConfig;

pub async fn send_booking_success(
    config: &EmailConfig,
    class_name: &str,
    time: &str,
    trainer: Option<&str>,
) {
    let trainer_str = trainer.unwrap_or("Not assigned");
    let subject = format!("Gym Booking Confirmed: {}", class_name);
    let body = format!(
        "Your gym class has been successfully booked!\n\n\
         Class: {}\n\
         Time: {}\n\
         Trainer: {}\n\n\
         See you there!",
        class_name, time, trainer_str
    );

    if let Err(e) = send_email(config, &subject, &body).await {
        error!("Failed to send success email: {}", e);
    } else {
        info!("Booking confirmation email sent");
    }
}

pub async fn send_booking_failure(
    config: &EmailConfig,
    class_name: &str,
    time: &str,
    trainer: Option<&str>,
    reason: &str,
) {
    let trainer_str = trainer.unwrap_or("Not assigned");
    let subject = format!("Gym Booking Failed: {}", class_name);
    let body = format!(
        "Failed to book your gym class.\n\n\
         Class: {}\n\
         Time: {}\n\
         Trainer: {}\n\n\
         Reason: {}\n\n\
         You may want to try booking manually or check the waitlist.",
        class_name, time, trainer_str, reason
    );

    if let Err(e) = send_email(config, &subject, &body).await {
        error!("Failed to send failure email: {}", e);
    } else {
        info!("Booking failure email sent");
    }
}

async fn send_email(config: &EmailConfig, subject: &str, body: &str) -> Result<(), String> {
    let email = Message::builder()
        .from(config.from.parse().map_err(|e| format!("Invalid from address: {}", e))?)
        .to(config.to.parse().map_err(|e| format!("Invalid to address: {}", e))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {}", e))?;

    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_server)
            .map_err(|e| format!("Failed to create SMTP transport: {}", e))?
            .port(config.smtp_port)
            .credentials(creds)
            .build();

    mailer
        .send(email)
        .await
        .map_err(|e| format!("Failed to send email: {}", e))?;

    Ok(())
}
