use std::process::{Command, Stdio, Child};
use std::path::PathBuf;
use anyhow::Result;
use uuid::Uuid;

use crate::isolation::IsolationEnvironment;

pub struct AppStreamer {
    pub process: Option<Child>,
    pub window_id: Option<u64>,
    pub isolation_env: Option<IsolationEnvironment>,
    pub command: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub isolate_files: bool,
}

impl AppStreamer {
    pub fn new(
        command: String,
        args: Vec<String>,
        workdir: Option<String>,
        isolate_files: bool,
        session_id: Uuid,
    ) -> Result<Self> {
        let mut streamer = Self {
            process: None,
            window_id: None,
            isolation_env: None,
            command,
            args,
            workdir,
            isolate_files,
        };

        if isolate_files {
            streamer.isolation_env = Some(IsolationEnvironment::new(session_id)?);
        }

        Ok(streamer)
    }

    pub fn start_application(&mut self) -> Result<()> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);
        
        if let Some(workdir) = &self.workdir {
            cmd.current_dir(workdir);
        }

        // Set up isolation environment if needed
        if let Some(ref isolation_env) = self.isolation_env {
            // TODO: copy any configured shared files into isolation dirs
            // isolation_env.copy_shared_files(&shared_paths)?;
            cmd.env("HOME", isolation_env.home_dir.as_os_str());
            cmd.env("XDG_DATA_HOME", isolation_env.data_dir.as_os_str());
            cmd.env("XDG_CONFIG_HOME", isolation_env.config_dir.as_os_str());
            cmd.env("XDG_CACHE_HOME", isolation_env.cache_dir.as_os_str());
            cmd.env("TMPDIR", isolation_env.temp_dir.as_os_str());
        }

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        log::info!("Starting application: {} with args: {:?}", self.command, self.args);
        
        let child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start application '{}': {}", self.command, e))?;

        self.process = Some(child);
        
        // Give the process a moment to start and create its window
        std::thread::sleep(std::time::Duration::from_millis(500));
        
        // Try to find the window ID
        if let Err(e) = self.find_window_id() {
            log::warn!("Could not find window ID for application: {}", e);
        }

        Ok(())
    }

    pub fn capture_window_frame(&mut self) -> Result<Option<Vec<u8>>> {
        if let Some(window_id) = self.window_id {
            self.capture_window_by_id(window_id)
        } else {
            // Try to find window again
            if self.find_window_id().is_ok() && self.window_id.is_some() {
                return self.capture_window_frame();
            }
            Ok(None)
        }
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut process) = self.process {
            match process.try_wait() {
                Ok(Some(_)) => {
                    log::info!("Application process has exited");
                    false
                }
                Ok(None) => true, // Still running
                Err(_) => false,  // Error checking status
            }
        } else {
            false
        }
    }

    pub fn stop_application(&mut self) -> Result<()> {
        if let Some(mut process) = self.process.take() {
            log::info!("Stopping application process");
            
            // Try graceful shutdown first
            process.kill().ok();
            
            // Wait a bit for graceful shutdown
            std::thread::sleep(std::time::Duration::from_millis(1000));
            
            // Force kill if still running
            let _ = process.wait();
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn find_window_id(&mut self) -> Result<()> {
        if let Some(ref process) = self.process {
            let pid = process.id();
            
            // Use xdotool to find window by PID
            let output = Command::new("xdotool")
                .args(&["search", "--pid", &pid.to_string()])
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    let window_ids = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_window) = window_ids.lines().next() {
                        if let Ok(window_id) = first_window.parse::<u64>() {
                            self.window_id = Some(window_id);
                            log::info!("Found window ID: {} for PID: {}", window_id, pid);
                            return Ok(());
                        }
                    }
                }
                Ok(_) => log::warn!("xdotool found no windows for PID: {}", pid),
                Err(e) => log::warn!("Failed to run xdotool: {}", e),
            }
        }
        
        Err(anyhow::anyhow!("Could not find window ID"))
    }

    #[cfg(target_os = "linux")]
    fn capture_window_by_id(&self, window_id: u64) -> Result<Option<Vec<u8>>> {
        // Use xwd to capture the window
        let output = Command::new("xwd")
            .args(&["-id", &window_id.to_string(), "-out", "/dev/stdout"])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                // Parse XWD format and convert to RGB
                if let Ok(rgb_data) = self.parse_xwd_to_rgb(&output.stdout) {
                    Ok(Some(rgb_data))
                } else {
                    log::warn!("Failed to parse XWD data for window {}", window_id);
                    Ok(None)
                }
            }
            Ok(_) => {
                log::warn!("xwd failed to capture window {}", window_id);
                Ok(None)
            }
            Err(e) => {
                log::error!("Failed to run xwd: {}", e);
                Ok(None)
            }
        }
    }

    // Simple XWD to RGB conversion (basic implementation)
    fn parse_xwd_to_rgb(&self, xwd_data: &[u8]) -> Result<Vec<u8>> {
        // XWD header is typically 100 bytes, but we'll do a basic check
        if xwd_data.len() < 100 {
            return Err(anyhow::anyhow!("XWD data too short"));
        }

        // Skip XWD header (simplified - real implementation would parse header properly)
        // For now, just return the data after the header as a placeholder
        // In production, you'd want proper XWD parsing or use a different capture method
        let header_size = 100; // Simplified assumption
        if xwd_data.len() > header_size {
            // Return the image data portion (this is a simplified approach)
            Ok(xwd_data[header_size..].to_vec())
        } else {
            Err(anyhow::anyhow!("Invalid XWD format"))
        }
    }
}

impl Drop for AppStreamer {
    fn drop(&mut self) {
        let _ = self.stop_application();
        
        if let Some(ref isolation_env) = self.isolation_env {
            if let Err(e) = isolation_env.cleanup() {
                log::error!("Failed to cleanup isolation environment: {}", e);
            }
        }
    }
}
