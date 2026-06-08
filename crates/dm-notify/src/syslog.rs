use dm_core::config::SyslogConfig;
use dm_core::event::FsEvent;
use std::net::UdpSocket;
use tracing::debug;

/// Sends syslog messages for filesystem events.
pub struct SyslogNotifier {
    config: SyslogNotifierConfig,
    socket: UdpSocket,
}

#[derive(Debug, Clone)]
struct SyslogNotifierConfig {
    server: String,
    port: u16,
    format: String,
    facility: u8,
    message_format: Option<String>,
}

impl SyslogNotifier {
    pub fn new(config: &SyslogConfig) -> Self {
        let facility = match config.facility.to_lowercase().as_str() {
            "kern" => 0,
            "user" => 1,
            "mail" => 2,
            "daemon" => 3,
            "auth" => 4,
            "syslog" => 5,
            "lpr" => 6,
            "news" => 7,
            "uucp" => 8,
            "cron" => 9,
            "authpriv" => 10,
            "ftp" => 11,
            "local0" => 16,
            "local1" => 17,
            "local2" => 18,
            "local3" => 19,
            "local4" => 20,
            "local5" => 21,
            "local6" => 22,
            "local7" => 23,
            _ => 1, // user
        };

        let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP socket");

        Self {
            config: SyslogNotifierConfig {
                server: config.server.clone(),
                port: config.port,
                format: config.format.clone(),
                facility,
                message_format: config.message_format.clone(),
            },
            socket,
        }
    }

    /// Send a syslog message for an event.
    pub fn notify(&self, event: &FsEvent) -> Result<(), String> {
        let default_format = "%event%: %path%";
        let template = self
            .config
            .message_format
            .as_deref()
            .unwrap_or(default_format);
        let message = event.format_with(template);

        // Priority = facility * 8 + severity (6 = info)
        let priority = self.config.facility * 8 + 6;
        let timestamp = event.timestamp.format("%b %d %H:%M:%S");

        let syslog_msg = match self.config.format.as_str() {
            "rfc3164" => format!(
                "<{}>{} directorymonitor[{}]: {}",
                priority,
                timestamp,
                std::process::id(),
                message
            ),
            _ => format!(
                // RFC5424
                "<{}>1 {} directorymonitor {} - - - {}",
                priority,
                event.timestamp.to_rfc3339(),
                std::process::id(),
                message
            ),
        };

        let addr = format!("{}:{}", self.config.server, self.config.port);
        self.socket
            .send_to(syslog_msg.as_bytes(), &addr)
            .map_err(|e| format!("Syslog send error: {e}"))?;

        debug!("Syslog sent: {}", message);
        Ok(())
    }
}
