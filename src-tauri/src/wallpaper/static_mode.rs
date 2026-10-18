//! Статичные обои через IDesktopWallpaper: на каждый монитор, с запоминанием исходных.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Исходные обои: путь монитора → путь к картинке.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Originals(pub Vec<(String, String)>);

#[cfg(windows)]
mod imp {
    use super::Originals;
    use std::path::Path;
    use windows::core::HSTRING;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};

    /// COM в отдельном STA-потоке на время одного вызова.
    fn with_wallpaper<T: Send + 'static>(
        f: impl FnOnce(&IDesktopWallpaper) -> windows::core::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        std::thread::spawn(move || unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let res = CoCreateInstance::<_, IDesktopWallpaper>(&DesktopWallpaper, None, CLSCTX_ALL)
                .and_then(|dw| f(&dw));
            if init.is_ok() {
                CoUninitialize();
            }
            res.map_err(|e| format!("IDesktopWallpaper: {e}"))
        })
        .join()
        .map_err(|_| "Сбой потока обоев".to_string())?
    }

    unsafe fn monitors(dw: &IDesktopWallpaper) -> windows::core::Result<Vec<String>> {
        let count = dw.GetMonitorDevicePathCount()?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let p = dw.GetMonitorDevicePathAt(i)?;
            out.push(p.to_string().unwrap_or_default());
            CoTaskMemFree(Some(p.0 as _));
        }
        Ok(out)
    }

    pub fn current() -> Result<Originals, String> {
        with_wallpaper(|dw| unsafe {
            let mut out = Vec::new();
            for m in monitors(dw)? {
                let path = dw.GetWallpaper(&HSTRING::from(m.as_str()))?;
                out.push((m, path.to_string().unwrap_or_default()));
                CoTaskMemFree(Some(path.0 as _));
            }
            Ok(Originals(out))
        })
    }

    pub fn set(image: &Path) -> Result<(), String> {
        let image = HSTRING::from(image.as_os_str());
        with_wallpaper(move |dw| unsafe {
            for m in monitors(dw)? {
                dw.SetWallpaper(&HSTRING::from(m.as_str()), &image)?;
            }
            Ok(())
        })
    }

    pub fn restore(o: &Originals) -> Result<(), String> {
        let o = o.clone();
        with_wallpaper(move |dw| unsafe {
            let present = monitors(dw)?;
            for (m, path) in &o.0 {
                if path.is_empty() || !present.contains(m) {
                    continue;
                }
                dw.SetWallpaper(&HSTRING::from(m.as_str()), &HSTRING::from(path.as_str()))?;
            }
            // Мониторы, подключённые позже, получают обои первого исходного.
            if let Some((_, first)) = o.0.iter().find(|(_, p)| !p.is_empty()) {
                for m in present
                    .iter()
                    .filter(|m| !o.0.iter().any(|(om, _)| om == *m))
                {
                    dw.SetWallpaper(&HSTRING::from(m.as_str()), &HSTRING::from(first.as_str()))?;
                }
            }
            Ok(())
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Originals;
    use std::path::Path;

    pub fn current() -> Result<Originals, String> {
        Err("Поддерживается только Windows".into())
    }
    pub fn set(_: &Path) -> Result<(), String> {
        Err("Поддерживается только Windows".into())
    }
    pub fn restore(_: &Originals) -> Result<(), String> {
        Err("Поддерживается только Windows".into())
    }
}

pub fn current() -> Result<Originals, String> {
    imp::current()
}

pub fn set(image: &Path) -> Result<(), String> {
    imp::set(image)
}

pub fn restore(originals: &Originals) -> Result<(), String> {
    imp::restore(originals)
}
