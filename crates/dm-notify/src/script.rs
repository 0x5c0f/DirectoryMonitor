use dm_core::event::FsEvent;
use std::process::Command;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, error, info};

/// Executes scripts or applications in response to filesystem events.
pub struct ScriptExecutor {
    /// Run scripts silently in background (no visible window on Windows).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    silent: bool,
}

impl ScriptExecutor {
    pub fn new(silent: bool) -> Self {
        Self { silent }
    }

    /// Execute a script/command with event context as arguments.
    /// The script receives macro-expanded arguments.
    pub async fn execute(
        &self,
        script: &str,
        event: &FsEvent,
        args_template: &[String],
    ) -> Result<(), String> {
        let expanded_args: Vec<String> = args_template
            .iter()
            .map(|arg| event.format_with(arg))
            .collect();

        info!(
            "Executing script: {} with args: {:?}",
            script, expanded_args
        );

        let mut cmd = AsyncCommand::new(script);
        cmd.args(&expanded_args);

        // On Windows, hide the console window
        #[cfg(target_os = "windows")]
        if self.silent {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.output().await {
            Ok(output) => {
                if output.status.success() {
                    debug!("Script completed successfully: {}", script);
                    if !output.stdout.is_empty() {
                        debug!("Script stdout: {}", String::from_utf8_lossy(&output.stdout));
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!("Script failed with {}: {}", output.status, stderr);
                    return Err(format!("Script exited with {}: {}", output.status, stderr));
                }
            }
            Err(e) => {
                error!("Failed to execute script '{}': {}", script, e);
                return Err(format!("Failed to execute '{script}': {e}"));
            }
        }

        Ok(())
    }

    /// Execute a script synchronously (blocking).
    pub fn execute_sync(
        &self,
        script: &str,
        event: &FsEvent,
        args_template: &[String],
    ) -> Result<(), String> {
        let expanded_args: Vec<String> = args_template
            .iter()
            .map(|arg| event.format_with(arg))
            .collect();

        let mut cmd = Command::new(script);
        cmd.args(&expanded_args);

        #[cfg(target_os = "windows")]
        if self.silent {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "Script exited with {}: {}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
            Err(e) => Err(format!("Failed to execute '{script}': {e}")),
        }
    }
}
