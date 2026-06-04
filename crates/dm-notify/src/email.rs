use dm_core::config::EmailConfig;
use dm_core::event::FsEvent;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info};

/// Sends email notifications for filesystem events.
pub struct EmailNotifier {
    config: EmailConfig,
    /// Pending events for batch mode.
    pending: Arc<Mutex<Vec<FsEvent>>>,
    /// Last send time for throttling.
    last_send: Arc<Mutex<Instant>>,
    /// Emails sent in the current minute (for rate limiting).
    sent_this_minute: Arc<Mutex<u32>>,
}

impl EmailNotifier {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            pending: Arc::new(Mutex::new(Vec::new())),
            last_send: Arc::new(Mutex::new(Instant::now())),
            sent_this_minute: Arc::new(Mutex::new(0)),
        }
    }

    /// Send a notification for a single event.
    pub async fn notify(&self, event: &FsEvent, recipients: &[String]) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        if self.config.batch_size > 0 {
            self.pending.lock().await.push(event.clone());
            let pending = self.pending.lock().await;
            if pending.len() >= self.config.batch_size {
                drop(pending);
                self.flush(recipients).await?;
            }
        } else {
            self.send_single(event, recipients).await?;
        }
        Ok(())
    }

    /// Flush all pending events as a single email.
    pub async fn flush(&self, recipients: &[String]) -> Result<(), String> {
        let events: Vec<FsEvent> = self.pending.lock().await.drain(..).collect();
        if events.is_empty() {
            return Ok(());
        }
        self.send_batch(&events, recipients).await
    }

    async fn send_single(&self, event: &FsEvent, recipients: &[String]) -> Result<(), String> {
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

    async fn send_batch(&self, events: &[FsEvent], recipients: &[String]) -> Result<(), String> {
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

    async fn send_email(&self, subject: &str, body: &str, recipients: &[String]) -> Result<(), String> {
        let credentials = Credentials::new(
            self.config.username.clone(),
            self.config.password.clone(),
        );

        let mailer = if self.config.use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp_server)
                .map_err(|e| format!("SMTP relay error: {e}"))?
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
                        .map_err(|e| format!("Invalid from address: {e}"))?,
                )
                .to(recipient
                    .parse()
                    .map_err(|e| format!("Invalid recipient '{recipient}': {e}"))?)
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string())
                .map_err(|e| format!("Failed to build email: {e}"))?;

            match mailer.send(email).await {
                Ok(_) => info!("Email sent to {}", recipient),
                Err(e) => error!("Failed to send email to {}: {}", recipient, e),
            }
        }

        *self.sent_this_minute.lock().await += 1;
        Ok(())
    }

    async fn check_rate_limit(&self) -> Result<(), String> {
        let mut sent = self.sent_this_minute.lock().await;
        let last = *self.last_send.lock().await;

        if last.elapsed() >= Duration::from_secs(60) {
            *sent = 0;
            *self.last_send.lock().await = Instant::now();
        }

        if *sent >= self.config.max_per_minute {
            return Err("Email rate limit exceeded".to_string());
        }

        Ok(())
    }
}
