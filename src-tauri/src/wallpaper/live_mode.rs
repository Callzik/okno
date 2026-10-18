//! Живой режим: окно без рамки за иконками рабочего стола (родитель — WorkerW).

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "live";

/// Аргументы WebView2 должны совпадать у всех окон приложения.
/// CalculateNativeWinOcclusion выключен, иначе WebView2 считает окно за иконками
/// скрытым и останавливает отрисовку.
pub const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion";

pub fn is_running(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

/// Создаёт окно сцены и сажает его за иконки. При ошибке окно закрывается.
pub async fn start(app: &AppHandle) -> Result<(), String> {
    if is_running(app) {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("live.html".into()))
        .title("Окно — живые обои")
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .focused(false)
        .visible(false)
        .additional_browser_args(BROWSER_ARGS)
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .build()
        .map_err(|e| format!("Не удалось создать окно сцены: {e}"))?;

    #[cfg(windows)]
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    #[cfg(not(windows))]
    let hwnd = 0isize;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(imp::attach(hwnd));
    })
    .map_err(|e| e.to_string())?;
    let res = rx
        .await
        .unwrap_or_else(|_| Err("Главный поток не ответил".into()));
    if res.is_err() {
        let _ = window.destroy();
    }
    res
}

pub fn stop(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}

#[cfg(windows)]
mod imp {
    use windows::core::{w, BOOL, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowExW, FindWindowW, GetSystemMetrics, SendMessageTimeoutW, SetParent,
        SetWindowPos, ShowWindow, HWND_TOP, SMTO_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_SHOWNOACTIVATE,
    };

    fn opt(h: windows::core::Result<HWND>) -> Option<HWND> {
        h.ok().filter(|h| !h.is_invalid())
    }

    unsafe extern "system" fn find_icons_host(top: HWND, lparam: LPARAM) -> BOOL {
        // Ищем окно верхнего уровня с SHELLDLL_DefView; нужный WorkerW идёт следом за ним.
        if opt(FindWindowExW(
            Some(top),
            None,
            w!("SHELLDLL_DefView"),
            PCWSTR::null(),
        ))
        .is_some()
        {
            if let Some(worker) = opt(FindWindowExW(
                None,
                Some(top),
                w!("WorkerW"),
                PCWSTR::null(),
            )) {
                *(lparam.0 as *mut HWND) = worker;
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    unsafe fn find_worker() -> Option<HWND> {
        let progman = opt(FindWindowW(w!("Progman"), PCWSTR::null()))?;
        // Просим Explorer создать WorkerW между обоями и иконками.
        for (wp, lp) in [(0xD, 0x1), (0, 0)] {
            SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(wp),
                LPARAM(lp),
                SMTO_NORMAL,
                1000,
                None,
            );
        }
        // Windows 11 24H2+: WorkerW — дочернее окно Progman.
        if let Some(w) = opt(FindWindowExW(
            Some(progman),
            None,
            w!("WorkerW"),
            PCWSTR::null(),
        )) {
            return Some(w);
        }
        // Классический вариант: WorkerW верхнего уровня после окна с иконками.
        let mut found = HWND::default();
        let _ = EnumWindows(
            Some(find_icons_host),
            LPARAM(&mut found as *mut HWND as isize),
        );
        (!found.is_invalid()).then_some(found)
    }

    pub fn attach(hwnd: isize) -> Result<(), String> {
        unsafe {
            let hwnd = HWND(hwnd as _);
            let worker = find_worker().ok_or("Не найдено окно WorkerW рабочего стола")?;
            SetParent(hwnd, Some(worker)).map_err(|e| format!("SetParent: {e}"))?;
            // Координаты внутри WorkerW отсчитываются от левого верхнего угла
            // виртуального рабочего стола, поэтому окно ставим в (0, 0).
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                0,
                0,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(|e| format!("SetWindowPos: {e}"))?;
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn attach(_: isize) -> Result<(), String> {
        Err("Живой режим работает только в Windows".into())
    }
}
