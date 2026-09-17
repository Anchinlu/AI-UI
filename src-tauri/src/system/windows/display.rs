//! system/windows/display.rs — Windows Monitor Brightness API.
//!
//! Uses DDC/CI via GetPhysicalMonitorsFromHMONITOR + GetMonitorBrightness.
//! EnumDisplayMonitors to enumerate all active monitors.
//! Fallback to WMI (wmi crate) for internal laptop displays that don't support DDC/CI.
//!
//! Must be called from a blocking thread (spawn_blocking).

use std::mem::{size_of, zeroed};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::{BOOL, LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use serde::Deserialize;

/// Information about a single display.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub brightness: Option<u8>,
    pub supports_brightness: bool,
    pub is_wmi: bool, // internal flag to know how to set it
}

/// Status of all displays.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DisplayStatus {
    pub monitors: Vec<DisplayInfo>,
}

/// Data passed through the EnumDisplayMonitors callback.
struct MonitorEnumData {
    hmonitors: Vec<(HMONITOR, MONITORINFOEXW)>,
}

/// Callback for EnumDisplayMonitors.
unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprc: *mut RECT,
    dwdata: LPARAM,
) -> BOOL {
    let data = &mut *(dwdata.0 as *mut MonitorEnumData);

    let mut info: MONITORINFOEXW = zeroed();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _).as_bool() {
        data.hmonitors.push((hmonitor, info));
    }

    TRUE // Continue enumeration
}

/// Enumerate monitors using EnumDisplayMonitors.
fn enumerate_monitors() -> Result<Vec<(HMONITOR, MONITORINFOEXW)>, String> {
    let mut enum_data = MonitorEnumData {
        hmonitors: Vec::new(),
    };

    unsafe {
        let result = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut enum_data as *mut MonitorEnumData as isize),
        );
        if !result.as_bool() {
            return Err("EnumDisplayMonitors failed".into());
        }
    }

    if enum_data.hmonitors.is_empty() {
        return Err("No monitors found".into());
    }

    Ok(enum_data.hmonitors)
}

/// Try to read brightness from a physical monitor via DDC/CI.
/// Returns (current_brightness_0_100, supports_brightness).
fn try_read_brightness_ddcci(hmonitor: HMONITOR) -> (Option<u8>, bool) {
    unsafe {
        let mut num_physical: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut num_physical).is_err()
            || num_physical == 0
        {
            return (None, false);
        }

        let mut physical_monitors = vec![PHYSICAL_MONITOR::default(); num_physical as usize];
        if GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut physical_monitors).is_err() {
            return (None, false);
        }

        let mut min: u32 = 0;
        let mut cur: u32 = 0;
        let mut max: u32 = 0;

        let success = GetMonitorBrightness(
            physical_monitors[0].hPhysicalMonitor,
            &mut min,
            &mut cur,
            &mut max,
        );

        let supports = success != 0;
        let brightness = if supports && max > min {
            let normalized = ((cur - min) as f64 / (max - min) as f64 * 100.0).round() as u8;
            Some(normalized.min(100))
        } else {
            None
        };

        let _ = DestroyPhysicalMonitors(&physical_monitors);

        (brightness, supports)
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct WmiMonitorBrightness {
    active: bool,
    current_brightness: u8,
}

fn try_read_brightness_wmi() -> Option<u8> {
    use wmi::{COMLibrary, WMIConnection};
    let com_con = COMLibrary::without_security().ok()?;
    let wmi_con = WMIConnection::with_namespace_path("ROOT\\WMI", com_con.into()).ok()?;
    
    let monitors: Vec<WmiMonitorBrightness> = wmi_con.query().ok()?;
    if let Some(first) = monitors.into_iter().find(|m| m.active) {
        return Some(first.current_brightness);
    }
    None
}

/// Get status of all connected displays.
pub fn get_display_status() -> Result<DisplayStatus, String> {
    let hmonitors = enumerate_monitors()?;
    let mut monitors = Vec::new();

    for (hmonitor, info) in &hmonitors {
        let device_name = String::from_utf16_lossy(&info.szDevice)
            .trim_matches(char::from(0))
            .to_string();

        let is_primary = (info.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1

        let (mut brightness, mut supports) = try_read_brightness_ddcci(*hmonitor);
        let mut is_wmi = false;

        // Fallback to WMI if it's the primary display and DDC/CI failed
        if !supports && is_primary {
            if let Some(wmi_brightness) = try_read_brightness_wmi() {
                brightness = Some(wmi_brightness);
                supports = true;
                is_wmi = true;
            }
        }

        monitors.push(DisplayInfo {
            id: device_name.clone(),
            name: if device_name.is_empty() {
                "Unknown Monitor".to_string()
            } else {
                device_name
            },
            is_primary,
            brightness,
            supports_brightness: supports,
            is_wmi,
        });
    }

    Ok(DisplayStatus { monitors })
}

/// Set brightness for a specific monitor identified by device name.
/// level: 0..=100
pub fn set_brightness(monitor_id: &str, level: u8) -> Result<(), String> {
    if level > 100 {
        return Err("Brightness level must be between 0 and 100".into());
    }

    let hmonitors = enumerate_monitors()?;

    // Find the target monitor to see if it is primary
    let target = hmonitors
        .iter()
        .find(|(_, info)| {
            let name = String::from_utf16_lossy(&info.szDevice)
                .trim_matches(char::from(0))
                .to_string();
            name == monitor_id
        })
        .ok_or_else(|| format!("Monitor '{}' not found", monitor_id))?;

    let is_primary = (target.1.monitorInfo.dwFlags & 1) != 0;

    // Try DDC/CI first
    unsafe {
        let mut num_physical: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(target.0, &mut num_physical).is_ok() && num_physical > 0 {
            let mut physical_monitors = vec![PHYSICAL_MONITOR::default(); num_physical as usize];
            if GetPhysicalMonitorsFromHMONITOR(target.0, &mut physical_monitors).is_ok() {
                let mut min: u32 = 0;
                let mut _cur: u32 = 0;
                let mut max: u32 = 0;

                if GetMonitorBrightness(physical_monitors[0].hPhysicalMonitor, &mut min, &mut _cur, &mut max) != 0 {
                    let target_value = min + ((level as u32) * (max - min) / 100);
                    let result = SetMonitorBrightness(physical_monitors[0].hPhysicalMonitor, target_value);
                    let _ = DestroyPhysicalMonitors(&physical_monitors);
                    
                    if result != 0 {
                        return Ok(());
                    }
                } else {
                    let _ = DestroyPhysicalMonitors(&physical_monitors);
                }
            }
        }
    }

    // If DDC/CI failed and it's primary, fallback to PowerShell WMI to set it
    // Using PowerShell avoids the complex IWbemServices::ExecMethod in Rust.
    if is_primary {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let script = format!("(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1,{})", level);
        let output = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Failed to execute powershell: {}", e))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("WMI fallback failed: {}", err_msg.trim()));
        }

        return Ok(());
    }

    Err("This monitor does not support brightness control".into())
}
