// Без консольного окна в релизной сборке Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod engine;
mod scene;
mod sun;
mod wallpaper;
mod weather;

use engine::{AppState, Mode};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

const MINIMIZED_ARG: &str = "--minimized";

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn quit(app: &AppHandle) {
    engine::restore(app);
    app.exit(0);
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Показать окно", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Пауза", true, false, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Обновить погоду", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &pause, &refresh, &sep, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Окно")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "pause" => {
                let paused = pause.is_checked().unwrap_or(false);
                engine::set_tray_paused(app, paused);
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    engine::tick(&a, false).await;
                });
            }
            "refresh" => {
                let a = app.clone();
                tauri::async_runtime::spawn(async move {
                    engine::tick(&a, true).await;
                });
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .manage(AppState {
            inner: Default::default(),
            client: weather::client(),
            tick_lock: Default::default(),
        })
        .invoke_handler(tauri::generate_handler![
            commands::search_city,
            commands::detect_city,
            commands::set_city,
            commands::get_state,
            commands::set_mode,
            commands::preview_scene,
            commands::set_custom_folder,
            commands::refresh_weather,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            setup_tray(&handle)?;

            if let Some(dir) = engine::bundled_scenes(&handle) {
                let _ = handle.asset_protocol_scope().allow_directory(dir, false);
            }
            let settings = engine::load_settings(&handle);
            if let Some(f) = &settings.custom_folder {
                let _ = handle.asset_protocol_scope().allow_directory(f, false);
            }
            let city = settings.city.clone();
            let mode = settings.mode;
            handle.state::<AppState>().lock().settings = settings;
            if let Some(c) = city.clone() {
                engine::set_city(&handle, c);
            }

            let minimized = std::env::args().any(|a| a == MINIMIZED_ARG);
            if !minimized || city.is_none() {
                show_main(&handle);
            }

            tauri::async_runtime::spawn(async move {
                if city.is_none() {
                    let client = handle.state::<AppState>().client.clone();
                    match weather::detect_city(&client).await {
                        Ok(c) => engine::set_city(&handle, c),
                        Err(e) => {
                            use tauri::Emitter;
                            let _ = handle.emit(
                                "weather-error",
                                serde_json::json!({ "message": format!("{e}. Выберите город вручную.") }),
                            );
                        }
                    }
                }
                if mode == Mode::Live {
                    engine::ensure_live(&handle).await;
                }
                engine::spawn_loops(&handle);
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Крестик сворачивает в трей.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("ошибка запуска");

    app.run(|app, event| match event {
        // Окно живой сцены может закрыться вместе с Explorer — приложение продолжает работать.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => engine::restore(app),
        _ => {}
    });
}
