use anyhow::Result;
use std::ffi::OsString;
use std::sync::mpsc;
use windows_service::{
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

use crate::runner::{run_monitor, run_serve};
use dm_core::config::AppConfig;

const SERVICE_NAME: &str = "DirectoryMonitor";
const SERVICE_DISPLAY_NAME: &str = "Directory Monitor";
const SERVICE_DESCRIPTION: &str = "Cross-platform filesystem monitoring service";

// Generate the FFI entry point
windows_service::define_windows_service!(ffi_service_main, service_main);

/// Entry point for Windows service mode.
pub fn run_service(_config: AppConfig, _config_path: &std::path::Path) -> Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

/// Service main function called by the SCM.
fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service_inner() {
        tracing::error!("Service failed: {}", e);
    }
}

/// Inner service implementation.
fn run_service_inner() -> Result<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, handler)?;

    // Parse command line arguments to get config path
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .iter()
        .position(|a| a == "-c" || a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("config.toml"));

    // Report that the service is starting
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(5),
        process_id: None,
    })?;

    // Load configuration
    let config = AppConfig::load(&config_path).unwrap_or_else(|e| {
        tracing::warn!(
            "Failed to load config from {}: {}, using defaults",
            config_path.display(),
            e
        );
        AppConfig::default()
    });

    // Create tokio runtime and run the monitor
    let rt = tokio::runtime::Runtime::new()?;

    // Report that the service is running
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    // Run the monitor in the tokio runtime
    let result = rt.block_on(async {
        tokio::select! {
            result = async {
                if config.server.enabled {
                    run_serve(config, config_path, &None).await
                } else {
                    run_monitor(config).await
                }
            } => result,
            _ = tokio::task::spawn_blocking(move || shutdown_rx.recv()) => {
                tracing::info!("Service stop signal received");
                Ok(())
            }
        }
    });

    // Report that the service is stopped
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::Win32(1)
        },
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    result
}

/// Install the Windows service.
pub fn install_service(config_path: &std::path::Path) -> Result<()> {
    use std::process::Command;

    let exe_path = std::env::current_exe()?;
    let config_abs = config_path
        .canonicalize()
        .unwrap_or_else(|_| config_path.to_path_buf());

    let bin_path = format!(
        "\"{}\" -c \"{}\" run-service",
        exe_path.display(),
        config_abs.display()
    );

    let output = Command::new("sc.exe")
        .args([
            "create",
            SERVICE_NAME,
            "binPath=",
            &bin_path,
            "start=",
            "auto",
            "DisplayName=",
            SERVICE_DISPLAY_NAME,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create service: {}", stderr);
    }

    // Set service description
    let _ = Command::new("sc.exe")
        .args(["description", SERVICE_NAME, SERVICE_DESCRIPTION])
        .output();

    tracing::info!("Service '{}' installed successfully.", SERVICE_NAME);
    tracing::info!("  Start with: sc start {}", SERVICE_NAME);
    tracing::info!("  Stop with:  sc stop {}", SERVICE_NAME);

    Ok(())
}

/// Uninstall the Windows service.
pub fn uninstall_service() -> Result<()> {
    use std::process::Command;

    // Stop the service first
    let _ = Command::new("sc.exe").args(["stop", SERVICE_NAME]).output();

    // Wait a moment for the service to stop
    std::thread::sleep(std::time::Duration::from_secs(2));

    let output = Command::new("sc.exe")
        .args(["delete", SERVICE_NAME])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to delete service: {}", stderr);
    }

    tracing::info!("Service '{}' removed successfully.", SERVICE_NAME);
    Ok(())
}
