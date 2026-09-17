//! system/windows/connectivity.rs — Connectivity Status (Wi-Fi, Bluetooth)
//!
//! Uses PowerShell and CMD scripts via std::process::Command to read system network state.
//! This avoids unstable WinRT API bindings in Win32 contexts.
//! Results are cached for 2 seconds to prevent process spawning spam.

use std::process::Command;
use std::os::windows::process::CommandExt;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, Duration};
use serde::Deserialize;
use crate::commands::{ConnectivityStatus, ConnectedDevice};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const CACHE_TTL: u64 = 2; // 2 seconds

static CACHE: OnceLock<Mutex<Option<(Instant, ConnectivityStatus)>>> = OnceLock::new();

fn get_cache() -> &'static Mutex<Option<(Instant, ConnectivityStatus)>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

#[derive(Deserialize)]
struct NetAdapter {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Status")]
    status: Option<String>,
    #[serde(rename = "InterfaceDescription")]
    _desc: Option<String>,
}

#[derive(Deserialize)]
struct BtService {
    #[serde(rename = "Status")]
    status: Option<u32>, // 4 = Running
}

#[derive(Deserialize)]
struct PnpDevice {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Status")]
    status: Option<String>,
    #[serde(rename = "Present")]
    present: Option<bool>,
}

pub fn get_connectivity_status() -> Result<ConnectivityStatus, String> {
    // 1. Check Cache
    if let Ok(cache) = get_cache().lock() {
        if let Some((timestamp, data)) = cache.as_ref() {
            if timestamp.elapsed() < Duration::from_secs(CACHE_TTL) {
                return Ok(data.clone());
            }
        }
    }

    // 2. Fetch data (this is blocking, but runs in spawn_blocking)
    let mut detected_devices = Vec::new();

    // -- Wi-Fi Status --
    let wifi_enabled = check_wifi_enabled();
    
    // -- Wi-Fi SSID --
    if wifi_enabled {
        if let Some(ssid) = get_connected_wifi_ssid() {
            detected_devices.push(ConnectedDevice {
                name: ssid,
                device_type: "wifi".to_string(),
                connection_state: "connected".to_string(),
            });
        }
    }

    // -- Bluetooth Status --
    let bluetooth_enabled = check_bluetooth_enabled();

    // -- Bluetooth Devices --
    if bluetooth_enabled {
        let mut bt_devices = get_bluetooth_devices();
        detected_devices.append(&mut bt_devices);
    }

    let status = ConnectivityStatus {
        wifi_enabled,
        bluetooth_enabled,
        detected_devices,
    };

    // 3. Update Cache
    if let Ok(mut cache) = get_cache().lock() {
        *cache = Some((Instant::now(), status.clone()));
    }

    Ok(status)
}

fn run_powershell<T: serde::de::DeserializeOwned>(script: String) -> Option<T> {
    use std::sync::mpsc::channel;
    use std::thread;

    let (tx, rx) = channel();
    thread::spawn(move || {
        let result = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(out)) => out,
        _ => return None, // timeout or error
    };

    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        // Handle PowerShell returning empty string or single object instead of array
        let json_str = json_str.trim();
        if json_str.is_empty() {
            return None;
        }
        
        // Sometimes it returns a single object, we might need array deserialization
        // But serde json can handle single objects if T is struct, or arrays if T is Vec.
        serde_json::from_str::<T>(json_str).ok()
    } else {
        None
    }
}

fn check_wifi_enabled() -> bool {
    let script = "Get-NetAdapter -ErrorAction SilentlyContinue | Select-Object -Property Name, Status, InterfaceDescription | ConvertTo-Json -Compress".to_string();
    
    // ConvertTo-Json might return a single object or an array. We read as serde_json::Value to handle both.
    if let Some(json) = run_powershell::<serde_json::Value>(script) {
        let adapters = if json.is_array() {
            json.as_array().unwrap().clone()
        } else if json.is_object() {
            vec![json]
        } else {
            vec![]
        };

        for adapter in adapters {
            if let Ok(adp) = serde_json::from_value::<NetAdapter>(adapter) {
                if let (Some(name), Some(status)) = (adp.name, adp.status) {
                    if name.to_lowercase().contains("wi-fi") || name.to_lowercase().contains("wifi") {
                        if status.to_lowercase() == "up" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    
    // Fallback: If Get-NetAdapter fails (e.g., cmdlets not available), assume false
    false
}

fn get_connected_wifi_ssid() -> Option<String> {
    use std::sync::mpsc::channel;
    use std::thread;

    let (tx, rx) = channel();
    thread::spawn(move || {
        let result = Command::new("cmd")
            .arg("/C")
            .arg("netsh wlan show interfaces")
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = tx.send(result);
    });

    let output = match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(out)) => out,
        _ => return None, // timeout or error
    };

    if output.status.success() {
        let out_str = String::from_utf8_lossy(&output.stdout);
        
        // Look for " State : connected" and then extract "SSID : <name>"
        let mut is_connected = false;
        for line in out_str.lines() {
            let line = line.trim();
            if line.starts_with("State") && line.contains("connected") {
                is_connected = true;
            } else if line.starts_with("State") && line.contains("disconnected") {
                is_connected = false;
            } else if is_connected && line.starts_with("SSID") && !line.starts_with("BSSID") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() > 1 {
                    return Some(parts[1].trim().to_string());
                }
            }
        }
    }
    None
}

fn check_bluetooth_enabled() -> bool {
    // Status 4 is Running
    let script = "Get-Service bthserv -ErrorAction SilentlyContinue | Select-Object -Property Status | ConvertTo-Json -Compress".to_string();
    if let Some(json) = run_powershell::<serde_json::Value>(script) {
        if let Ok(svc) = serde_json::from_value::<BtService>(json) {
            if let Some(status) = svc.status {
                return status == 4;
            }
        }
    }
    false
}

fn get_bluetooth_devices() -> Vec<ConnectedDevice> {
    let mut devices = Vec::new();
    let script = "Get-PnpDevice -Class Bluetooth -ErrorAction SilentlyContinue | Select-Object -Property Name, Status, Present | ConvertTo-Json -Compress".to_string();
    
    if let Some(json) = run_powershell::<serde_json::Value>(script) {
        let pnp_list = if json.is_array() {
            json.as_array().unwrap().clone()
        } else if json.is_object() {
            vec![json]
        } else {
            vec![]
        };

        for item in pnp_list {
            if let Ok(device) = serde_json::from_value::<PnpDevice>(item) {
                if let (Some(name), Some(status), Some(present)) = (device.name, device.status, device.present) {
                    let lname = name.to_lowercase();
                    // Filter out non-peripheral system devices
                    if present 
                        && status.to_lowercase() == "ok"
                        && !lname.contains("enumerator")
                        && !lname.contains("adapter") 
                        && !lname.contains("rfcomm") 
                    {
                        // Clean up generic trailing parts like " Avrcp Transport"
                        let clean_name = name.replace(" Avrcp Transport", "");
                        
                        // Avoid duplicates if there are multiple transports for the same device
                        if !devices.iter().any(|d: &ConnectedDevice| d.name == clean_name) {
                            devices.push(ConnectedDevice {
                                name: clean_name,
                                device_type: "bluetooth".to_string(),
                                connection_state: "detected".to_string(), // As requested by user, not "connected"
                            });
                        }
                    }
                }
            }
        }
    }
    
    devices
}
