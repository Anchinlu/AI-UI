//! system/windows/audio.rs — Windows Core Audio API (WASAPI) integration.
//!
//! Uses COM interfaces IMMDeviceEnumerator and IAudioEndpointVolume
//! to read/write system audio state. Each call:
//! 1. Initializes COM on the current thread (via RAII ComGuard)
//! 2. Gets the default audio render endpoint (never cached)
//! 3. Performs the operation
//! 4. Uninitializes COM on drop
//!
//! Must be called from a blocking thread (spawn_blocking), never from async directly.

use windows::core::GUID;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    Endpoints::IAudioEndpointVolume,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    STGM_READ,
};

/// Audio status returned to the frontend.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AudioStatus {
    pub volume: u8,
    pub muted: bool,
    pub output_device: Option<String>,
}

/// RAII guard for COM initialization.
/// Calls CoUninitialize on drop only when we successfully initialized.
struct ComGuard {
    should_uninit: bool,
}

impl ComGuard {
    fn init() -> Result<Self, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            match hr.0 {
                0 => Ok(ComGuard { should_uninit: true }),  // S_OK — first init
                1 => Ok(ComGuard { should_uninit: true }),  // S_FALSE — already init'd same mode, balance with CoUninitialize
                hr_val if hr_val == -2147417850i32 => {
                    // RPC_E_CHANGED_MODE (0x80010106)
                    // COM already initialized in a different apartment model.
                    // We can still use COM objects, but must NOT call CoUninitialize.
                    Ok(ComGuard { should_uninit: false })
                }
                hr_val => {
                    Err(format!("CoInitializeEx failed: HRESULT 0x{:08X}", hr_val as u32))
                }
            }
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.should_uninit {
            unsafe { CoUninitialize(); }
        }
    }
}

/// Helper: read the friendly name from an IMMDevice.
/// Returns None on any failure (never panics or fails the caller).
fn read_device_friendly_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let prop = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        // PROPVARIANT in windows-core 0.58 can be converted to string
        // via its Display or Debug trait, but the safest way is to
        // read the raw pwszVal pointer.
        //
        // The PROPVARIANT for PKEY_Device_FriendlyName is VT_LPWSTR.
        // In windows 0.58, PROPVARIANT is an opaque type in windows_core.
        // We use the to_string() method if available, otherwise try manual extraction.
        
        // Try using the Display implementation on PROPVARIANT
        let name = format!("{}", prop);
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() || trimmed == "PROPVARIANT(Empty)" {
            None
        } else {
            Some(trimmed)
        }
    }
}

/// Helper: get the default audio render endpoint and its volume interface.
fn get_default_endpoint() -> Result<(IAudioEndpointVolume, Option<String>), String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Failed to create IMMDeviceEnumerator: {}", e))?;

        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("No default audio endpoint available: {}", e))?;

        let friendly_name = read_device_friendly_name(&device);

        let endpoint_volume: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Failed to activate IAudioEndpointVolume: {}", e))?;

        Ok((endpoint_volume, friendly_name))
    }
}

/// Read current audio status from the system default device.
pub fn get_audio_status() -> Result<AudioStatus, String> {
    let _com = ComGuard::init()?;
    let (endpoint_volume, friendly_name) = get_default_endpoint()?;

    unsafe {
        let scalar = endpoint_volume
            .GetMasterVolumeLevelScalar()
            .map_err(|e| format!("Failed to get volume level: {}", e))?;

        let muted_raw = endpoint_volume
            .GetMute()
            .map_err(|e| format!("Failed to get mute state: {}", e))?;

        Ok(AudioStatus {
            volume: (scalar * 100.0).round() as u8,
            muted: muted_raw.as_bool(),
            output_device: friendly_name,
        })
    }
}

/// Set master volume (0..=100).
pub fn set_volume(level: u8) -> Result<(), String> {
    if level > 100 {
        return Err("Volume level must be between 0 and 100".into());
    }

    let _com = ComGuard::init()?;
    let (endpoint_volume, _) = get_default_endpoint()?;

    unsafe {
        let scalar = level as f32 / 100.0;
        let guid = &GUID::zeroed() as *const GUID;
        endpoint_volume
            .SetMasterVolumeLevelScalar(scalar, guid)
            .map_err(|e| format!("Failed to set volume: {}", e))?;
    }

    Ok(())
}

/// Set mute state.
pub fn set_mute(muted: bool) -> Result<(), String> {
    let _com = ComGuard::init()?;
    let (endpoint_volume, _) = get_default_endpoint()?;

    unsafe {
        let guid = &GUID::zeroed() as *const GUID;
        endpoint_volume
            .SetMute(muted, guid)
            .map_err(|e| format!("Failed to set mute: {}", e))?;
    }

    Ok(())
}
