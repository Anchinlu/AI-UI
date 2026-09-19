//! provider/llama_server.rs — Quản lý vòng đời tiến trình llama-server.exe
//!
//! Chức năng:
//! - Khởi chạy tiến trình ẩn (No Console) bằng Windows API.
//! - Tìm port rảnh trên localhost và tự động retry nếu xung đột.
//! - Đọc log stdout/stderr không đồng bộ (không block pipe).
//! - Health check loop chờ server sẵn sàng.
//! - Đảm bảo dọn dẹp tiến trình con khi bị Drop.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use reqwest::Client;

use crate::provider::ProviderError;

pub struct LlamaServerManager {
    child: Option<Child>,
    pub port: u16,
    pub process_id: u32,
    pub model_path: String,
    job_handle: Option<usize>,
}

impl Drop for LlamaServerManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id();
            log::info!("LlamaServerManager: Shutting down sidecar (PID: {})...", pid);
            
            // Kill process
            let _ = child.kill();
            
            // Wait for it to fully exit to prevent zombies
            match child.wait() {
                Ok(status) => {
                    log::info!("LlamaServerManager: Sidecar (PID: {}) exited with {}", pid, status);
                }
                Err(e) => {
                    log::warn!("LlamaServerManager: Error waiting for sidecar (PID: {}): {}", pid, e);
                }
            }
        }

        #[cfg(windows)]
        {
            if let Some(handle) = self.job_handle {
                use windows::Win32::Foundation::{HANDLE, CloseHandle};
                unsafe {
                    let _ = CloseHandle(HANDLE(handle as *mut std::ffi::c_void));
                }
            }
        }
    }
}

impl LlamaServerManager {
    /// Khởi chạy llama-server. Nếu port được cấp bị lỗi, sẽ tự động retry tối đa 3 lần.
    pub async fn start(
        exe_path: &str,
        model_path: &str,
        num_ctx: u32,
    ) -> Result<Self, ProviderError> {
        let max_retries = 3;
        let mut attempt = 0;

        loop {
            attempt += 1;
            
            let port = Self::get_free_port().map_err(|e| {
                ProviderError::ConnectionFailed(format!("Khong the tim port ranh: {}", e))
            })?;

            log::info!(
                "LlamaServerManager: Attempt {} - Starting llama-server on port {}...",
                attempt, port
            );

            match Self::spawn_process(exe_path, model_path, port, num_ctx) {
                Ok((child, pid)) => {
                    let job_handle = assign_to_job_object(&child);
                    if job_handle.is_none() {
                        #[cfg(windows)]
                        log::warn!("LlamaServerManager: Cảnh báo - Không thể gán sidecar vào Job Object. Tiến trình có thể không tự tắt nếu app crash.");
                    }
                    let mut manager = Self {
                        child: Some(child),
                        port,
                        process_id: pid,
                        model_path: model_path.to_string(),
                        job_handle,
                    };
                    
                    // Check health. If cancelled here, manager is dropped and child is killed!
                    if manager.wait_for_health(Duration::from_secs(30)).await {
                        log::info!("LlamaServerManager: Health check OK. Sidecar is ready.");
                        return Ok(manager);
                    } else {
                        log::error!("LlamaServerManager: Health check failed for PID: {}.", pid);
                        // manager is dropped here, killing the child before retry.

                        if attempt >= max_retries {
                            return Err(ProviderError::ConnectionFailed(
                                "Llama server health check failed after max retries.".into(),
                            ));
                        }
                    }
                }
                Err(e) => {
                    log::error!("LlamaServerManager: Failed to spawn process: {}", e);
                    if attempt >= max_retries {
                        return Err(ProviderError::ConnectionFailed(format!(
                            "Failed to spawn sidecar: {}", e
                        )));
                    }
                }
            }
            
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Tự động lấy số luồng tối ưu (Physical Cores - 2, fallback = 8)
    fn get_optimal_threads() -> u32 {
        #[cfg(windows)]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("wmic")
                .args(&["cpu", "get", "NumberOfCores"])
                .output()
            {
                let out_str = String::from_utf8_lossy(&output.stdout);
                for line in out_str.lines().skip(1) {
                    if let Ok(cores) = line.trim().parse::<u32>() {
                        return if cores > 2 { cores - 2 } else { cores };
                    }
                }
            }
        }
        8
    }

    /// Lấy một port TCP trống trên 127.0.0.1
    fn get_free_port() -> std::io::Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        // listener sẽ bị drop ở đây và trả lại port cho OS
        Ok(port)
    }

    /// Chạy tiến trình con bằng std::process::Command và bọc stdout/stderr vào background threads.
    fn spawn_process(
        exe_path: &str,
        model_path: &str,
        port: u16,
        num_ctx: u32,
    ) -> Result<(Child, u32), std::io::Error> {
        let mut cmd = Command::new(exe_path);
        let threads = Self::get_optimal_threads().to_string();
        
        cmd.args([
            "-m", model_path,
            "--port", &port.to_string(),
            "-ngl", "0",
            "-c", &num_ctx.to_string(),
            "-t", &threads,
            "-tb", &threads,
        ]);

        // Hide console on Windows
        #[cfg(target_os = "windows")]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        // Pipe I/O
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // Avoid inheritance of stdin
        cmd.stdin(Stdio::null());

        let mut child = cmd.spawn()?;
        let pid = child.id();

        // Spawn background threads to consume stdout/stderr so buffers don't fill up and block.
        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        // Tránh ghi log tràn lan, chỉ log debug
                        log::debug!("[llama-server stdout] {}", l);
                    }
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(l) = line {
                        // llama-server thường in thông tin init ra stderr
                        log::debug!("[llama-server stderr] {}", l);
                    }
                }
            });
        }

        Ok((child, pid))
    }

    /// Vòng lặp poll API health check.
    async fn wait_for_health(&mut self, timeout: Duration) -> bool {
        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_default();
            
        let url = format!("http://127.0.0.1:{}/health", self.port);
        let start = Instant::now();

        while start.elapsed() < timeout {
            if let Some(child) = &mut self.child {
                if let Ok(Some(status)) = child.try_wait() {
                    log::error!("LlamaServerManager: Sidecar exited prematurely: {}", status);
                    return false;
                }
            }

            if let Ok(res) = client.get(&url).send().await {
                if res.status().is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>().await {
                        if json.get("status").and_then(|s| s.as_str()) == Some("ok") {
                            return true;
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_free_port() {
        let port1 = LlamaServerManager::get_free_port().unwrap();
        let port2 = LlamaServerManager::get_free_port().unwrap();
        // port > 0
        assert!(port1 > 0);
        assert!(port2 > 0);
    }

    #[test]
    #[ignore = "Manual integration test with actual llama-server"]
    fn test_llama_server_manager_integration() {
        tauri::async_runtime::block_on(async {
            let exe_path = r#"E:\UI AI\ai-taskbar\llama-cpp\llama-server.exe"#;
            let model_path = r#"C:\Users\lctan\.ollama\models\blobs\sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"#;
            
            let manager = LlamaServerManager::start(exe_path, model_path, 4096).await.expect("Failed to start manager");
            
            assert!(manager.port > 0);
            assert!(manager.process_id > 0);
            
            let res = reqwest::get(format!("http://127.0.0.1:{}/health", manager.port)).await.unwrap();
            assert!(res.status().is_success());
            
            let output = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", manager.process_id)])
                .output()
                .unwrap();
            let output_str = String::from_utf8_lossy(&output.stdout);
            assert!(output_str.contains(&manager.process_id.to_string()));
            assert!(output_str.contains("llama-server.exe"));
            
            let pid = manager.process_id;
            drop(manager);
            
            std::thread::sleep(std::time::Duration::from_millis(500));
            
            let output_after = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", pid)])
                .output()
                .unwrap();
            let output_str_after = String::from_utf8_lossy(&output_after.stdout);
            assert!(!output_str_after.contains("llama-server.exe"));
        });
    }
}

#[cfg(windows)]
fn assign_to_job_object(child: &std::process::Child) -> Option<usize> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{HANDLE, CloseHandle};
    use windows::Win32::System::JobObjects::{
        CreateJobObjectW, AssignProcessToJobObject, SetInformationJobObject,
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        let job = match CreateJobObjectW(None, None) {
            Ok(h) => h,
            Err(_) => return None,
        };
        
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        
        let res = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        if res.is_ok() {
            let child_handle = HANDLE(child.as_raw_handle() as *mut _);
            if AssignProcessToJobObject(job, child_handle).is_ok() {
                return Some(job.0 as usize);
            }
        }
        
        let _ = CloseHandle(job);
        None
    }
}

#[cfg(not(windows))]
fn assign_to_job_object(_child: &std::process::Child) -> Option<usize> {
    None
}
