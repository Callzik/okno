//! Tauri-команды для окна настроек.

use crate::engine::{self, AppState, Mode, SceneState};
use crate::scene::Scene;
use crate::weather::{self, City};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn search_city(state: State<'_, AppState>, query: String) -> Result<Vec<City>, String> {
    weather::search_city(&state.client, &query).await
}

#[tauri::command]
pub async fn detect_city(state: State<'_, AppState>) -> Result<City, String> {
    weather::detect_city(&state.client).await
}

#[tauri::command]
pub async fn set_city(app: AppHandle, city: City) -> Result<SceneState, String> {
    engine::set_city(&app, city);
    Ok(engine::tick(&app, true).await)
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> SceneState {
    engine::state(&app)
}

#[tauri::command]
pub async fn set_mode(app: AppHandle, mode: String) -> Result<SceneState, String> {
    let mode = match mode.as_str() {
        "static" => Mode::Static,
        "live" => Mode::Live,
        _ => return Err(format!("Неизвестный режим: {mode}")),
    };
    Ok(engine::set_mode(&app, mode).await)
}

/// Ручной выбор сцены для проверки; `None` возвращает реальную.
#[tauri::command]
pub async fn preview_scene(app: AppHandle, scene: Option<String>) -> Result<SceneState, String> {
    let scene = match scene.as_deref() {
        None | Some("") => None,
        Some(key) => Some(Scene::parse(key).ok_or_else(|| format!("Неизвестная сцена: {key}"))?),
    };
    app.state::<AppState>().lock().preview = scene;
    Ok(engine::tick(&app, false).await)
}

#[tauri::command]
pub async fn set_custom_folder(
    app: AppHandle,
    folder: Option<String>,
) -> Result<SceneState, String> {
    let folder = folder.filter(|f| !f.trim().is_empty());
    if let Some(f) = &folder {
        let path = std::path::Path::new(f);
        if !path.is_dir() {
            return Err("Папка не найдена".into());
        }
        // Чтобы превью могло показать картинки из этой папки.
        let _ = app.asset_protocol_scope().allow_directory(path, false);
    }
    app.state::<AppState>().lock().settings.custom_folder = folder;
    engine::save_settings(&app);
    Ok(engine::tick(&app, false).await)
}

#[tauri::command]
pub async fn refresh_weather(app: AppHandle) -> SceneState {
    engine::tick(&app, true).await
}
