use dm_core::config::EmailConfig;
use dm_core::error::NotificationError;
use dm_core::event::FsEvent;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info};

/// Rate limiter state for email sending.
struct RateLimiter {
    last_send: Instant,
    sent_this_minute: u32,
}

/// Sends email notifications for filesystem events.
pub struct EmailNotifier {
    config: EmailConfig,
    /// Pending events for batch mode.
    pending: Arc<Mutex<Vec<FsEvent>>>,
    /// Rate limiter state (last_send + sent_this_minute under single lock).
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

impl EmailNotifier {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            pending: Arc::new(Mutex::new(Vec::new())),
            rate_limiter: Arc::new(Mutex::new(RateLimiter {
                last_send: Instant::now(),
                sent_this_minute: 0,
            })),
        }
    }

    /// Send a notification for a single event.
    pub async fn notify(
        &self,
        event: &FsEvent,
        recipients: &[String],
    ) -> Result<(), NotificationError> {
        if !self.config.enabled {
            return Ok(());
        }

        if self.config.batch_size > 0 {
            let mut pending = self.pending.lock().await;
            pending.push(event.clone());
            let should_flush = pending.len() >= self.config.batch_size;
            drop(pending);
            if should_flush {
                self.flush(recipients).await?;
            }
        } else {
            self.send_single(event, recipients).await?;
        }
        Ok(())
    }

    /// Flush all pending events as a single email.
    pub async fn flush(&self, recipients: &[String]) -> Result<(), NotificationError> {
        let events: Vec<FsEvent> = self.pending.lock().await.drain(..).collect();
        if events.is_empty() {
            return Ok(());
        }
        self.send_batch(&events, recipients).await
    }

    async fn send_single(
        &self,
        event: &FsEvent,
        recipients: &[String],
    ) -> Result<(), NotificationError> {
        self.check_rate_limit().await?;

        let subject = format!("[DirMon] {} - {}", event.event_type, event.path.display());
        let body = format!(
            "Event: {}\nPath: {}\nTimestamp: {}\nUser: {}\nProcess: {}",
            event.event_type,
            event.path.display(),
            event.timestamp,
            event.user.as_deref().unwrap_or("unknown"),
            event.process.as_deref().unwrap_or("unknown"),
        );

        self.send_email(&subject, &body, recipients).await
    }

    async fn send_batch(
        &self,
        events: &[FsEvent],
        recipients: &[String],
    ) -> Result<(), NotificationError> {
        self.check_rate_limit().await?;

        let subject = format!("[DirMon] {} events detected", events.len());
        let mut body = String::from("Filesystem events:\n\n");
        for event in events {
            body.push_str(&format!(
                "[{}] {} - {} ({})\n",
                event.timestamp.format("%H:%M:%S"),
                event.event_type,
                event.path.display(),
                event.user.as_deref().unwrap_or("unknown"),
            ));
        }

        self.send_email(&subject, &body, recipients).await
    }

    async fn send_email(
        &self,
        subject: &str,
        body: &str,
        recipients: &[String],
    ) -> Result<(), NotificationError> {
        let credentials =
            Credentials::new(self.config.username.clone(), self.config.password.clone());

        let mailer = if self.config.use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp_server)
                .map_err(|e| NotificationError::Email(Box::new(e)))?
                .credentials(credentials)
                .port(self.config.smtp_port)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.smtp_server)
                .credentials(credentials)
                .port(self.config.smtp_port)
                .build()
        };

        for recipient in recipients {
            let email = Message::builder()
                .from(
                    self.config
                        .from_address
                        .parse()
                        .map_err(|e| NotificationError::Email(Box::new(e)))?,
                )
                .to(recipient
                    .parse()
                    .map_err(|e| NotificationError::Email(Box::new(e)))?)
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string())
                .map_err(|e| NotificationError::Email(Box::new(e)))?;

            match mailer.send(email).await {
                Ok(_) => info!("Email sent to {}", recipient),
                Err(e) => error!("Failed to send email to {}: {}", recipient, e),
            }
        }

        let mut limiter = self.rate_limiter.lock().await;
        limiter.sent_this_minute += 1;
        Ok(())
    }

    async fn check_rate_limit(&self) -> Result<(), NotificationError> {
        let mut limiter = self.rate_limiter.lock().await;

        if limiter.last_send.elapsed() >= Duration::from_secs(60) {
            limiter.sent_this_minute = 0;
            limiter.last_send = Instant::now();
        }

        if limiter.sent_this_minute >= self.config.max_per_minute {
            return Err(NotificationError::Email(Box::new(std::io::Error::other(
                "Email rate limit exceeded",
            ))));
        }

        Ok(())
    }
}
