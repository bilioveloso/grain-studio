//! Full-screen photo view turns the laptop screen to full brightness and puts it back afterwards.
//! Windows only, through the WMI brightness class that built-in laptop panels expose; external
//! monitors don't support it, so failures are ignored.

use std::sync::Mutex;

/// Brightness before full screen, so leaving restores exactly what the user had.
static SAVED: Mutex<Option<u8>> = Mutex::new(None);

#[cfg(windows)]
fn powershell(script: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(windows)]
fn current_brightness() -> Option<u8> {
    powershell("(Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightness | Select-Object -First 1).CurrentBrightness")?
        .parse()
        .ok()
}

#[cfg(windows)]
fn set_brightness(level: u8) -> bool {
    powershell(&format!(
        "Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightnessMethods | \
         Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=0; Brightness={level}}} | Out-Null"
    ))
    .is_some()
}

#[cfg(not(windows))]
fn current_brightness() -> Option<u8> {
    None
}

#[cfg(not(windows))]
fn set_brightness(_level: u8) -> bool {
    false
}

/// Remember the current brightness and turn it to 100%. Returns whether the screen supports it.
/// ponytail: if the app closes while in full screen, brightness stays at 100%; restore on exit if that bothers anyone.
#[tauri::command]
pub async fn boost_brightness() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(current) = current_brightness() else { return false };
        let mut saved = SAVED.lock().unwrap();
        // Entering twice must not overwrite the real original with 100.
        if saved.is_none() {
            *saved = Some(current);
        }
        // Lock held through the change, so a quick exit restores after this boost, not before it.
        set_brightness(100)
    })
    .await
    .unwrap_or(false)
}

/// Put back the brightness saved by `boost_brightness`.
#[tauri::command]
pub async fn restore_brightness() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        let mut saved = SAVED.lock().unwrap();
        match saved.take() {
            Some(level) => set_brightness(level),
            None => false,
        }
    })
    .await
    .unwrap_or(false)
}
