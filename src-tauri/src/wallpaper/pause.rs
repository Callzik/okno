//! Когда ставить живую сцену на паузу: полноэкранное приложение, развёрнутое окно
//! или работа от батареи.

#[cfg(windows)]
pub fn fullscreen_app() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE,
        QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    match unsafe { SHQueryUserNotificationState() } {
        Ok(s) => s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE,
        Err(_) => false,
    }
}

#[cfg(windows)]
pub fn on_battery() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    // ACLineStatus: 0 — от батареи, 1 — от сети, 255 — неизвестно (ПК без батареи).
    unsafe { GetSystemPowerStatus(&mut s) }.is_ok() && s.ACLineStatus == 0
}

/// Есть ли развёрнутое окно или окно на весь монитор (в том числе игра без рамки).
#[cfg(windows)]
pub fn maximized_window() -> bool {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, IsZoomed, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TRANSPARENT,
    };

    unsafe extern "system" fn check(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = &mut *(lparam.0 as *mut bool);
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }
        // Служебные и прозрачные окна-оверлеи (счётчики FPS, оверлеи драйверов) не считаем.
        let skip = WS_EX_TOOLWINDOW.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0;
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & skip != 0 {
            return BOOL(1);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return BOOL(1);
        }
        // Окна на других виртуальных столах и «призраки» UWP скрыты через DWM.
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return BOOL(1);
        }
        let mut class = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..len]);
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return BOOL(1);
        }
        if IsZoomed(hwnd).as_bool() {
            *found = true;
            return BOOL(0);
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut r = RECT::default();
        if !monitor.is_invalid()
            && GetMonitorInfoW(monitor, &mut info).as_bool()
            && GetWindowRect(hwnd, &mut r).is_ok()
        {
            let m = info.rcMonitor;
            if r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom {
                *found = true;
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    let mut found = false;
    unsafe {
        let _ = EnumWindows(Some(check), LPARAM(&mut found as *mut bool as isize));
    }
    found
}

#[cfg(not(windows))]
pub fn maximized_window() -> bool {
    false
}

#[cfg(not(windows))]
pub fn fullscreen_app() -> bool {
    false
}

#[cfg(not(windows))]
pub fn on_battery() -> bool {
    false
}

/// Причина автоматической паузы, если она нужна.
pub fn auto_reason() -> Option<&'static str> {
    if fullscreen_app() {
        Some("fullscreen")
    } else if maximized_window() {
        Some("maximized")
    } else if on_battery() {
        Some("battery")
    } else {
        None
    }
}
