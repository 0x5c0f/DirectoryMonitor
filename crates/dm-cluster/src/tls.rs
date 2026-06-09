use dm_core::config::TlsConfig;
use tracing::info;

/// TLS configuration for cluster connections.
pub struct ClusterTls {
    pub enabled: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
    pub verify_client: bool,
}

impl ClusterTls {
    /// Create from config.
    pub fn from_config(config: &TlsConfig) -> Self {
        Self {
            enabled: config.enabled,
            cert_file: config.cert_file.clone(),
            key_file: config.key_file.clone(),
            ca_file: config.ca_file.clone(),
            verify_client: config.verify_client,
        }
    }

    /// Check if TLS is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Validate TLS configuration.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if self.cert_file.is_none() {
            return Err("TLS enabled but no cert_file configured".to_string());
        }

        if self.key_file.is_none() {
            return Err("TLS enabled but no key_file configured".to_string());
        }

        info!("TLS configuration validated");
        Ok(())
    }
}
