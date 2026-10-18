//! Состояние приложения и главный цикл: погода → сцена → обои.

use crate::scene::Scene;
use crate::sun::{self, Phase};
use crate::wallpaper::{live_mode, pause, static_mode};
use crate::weather::{self, City, Weather, WeatherKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

const WEATHER_PERIOD: i64 = 20 * 60;
const WEATHER_RETRY: i64 = 2 * 60;
const STORE: &str = "settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Static,
    Live,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub city: Option<City>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub custom_folder: Option<String>,
}

#[derive(Default)]
pub struct Inner {
    pub settings: Settings,
    pub weather: Option<Weather>,
    /// Последний запрос погоды не удался, показываем кэш.
    pub offline: bool,
    pub next_fetch: i64,
    pub preview: Option<Scene>,
    /// Картинка, которая сейчас стоит на обоях.
    pub applied: Option<PathBuf>,
    pub tray_paused: bool,
    pub auto_pause: Option<&'static str>,
    pub warning: Option<String>,
    pub originals: Option<static_mode::Originals>,
}

pub struct AppState {
    pub inner: Mutex<Inner>,
    pub client: reqwest::Client,
    /// Не даёт двум обновлениям идти одновременно.
    pub tick_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Данные для живой сцены (событие scene-update).
#[derive(Debug, Clone, Serialize)]
pub struct SceneUpdate {
    pub phase: Phase,
    pub weather: WeatherKind,
    pub sun_elevation: f64,
    pub cloud_cover: f64,
    pub temperature: Option<f64>,
    /// Фото сцены: в живом режиме это фон, поверх рисуются осадки.
    pub image_path: Option<String>,
}

/// Полное состояние для окна настроек.
#[derive(Debug, Clone, Serialize)]
pub struct SceneState {
    pub city: Option<City>,
    pub scene: Option<String>,
    pub phase: Option<Phase>,
    pub weather: Option<WeatherKind>,
    pub sun_elevation: Option<f64>,
    pub cloud_cover: Option<f64>,
    pub temperature: Option<f64>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub updated_at: Option<String>,
    pub mode: Mode,
    pub custom_folder: Option<String>,
    pub preview: Option<String>,
    pub paused: bool,
    pub pause_reason: Option<String>,
    pub offline: bool,
    pub warning: Option<String>,
    pub image_path: Option<String>,
}

impl SceneState {
    pub fn update(&self) -> Option<SceneUpdate> {
        Some(SceneUpdate {
            phase: self.phase?,
            weather: self.weather?,
            sun_elevation: self.sun_elevation?,
            cloud_cover: self.cloud_cover.unwrap_or(0.0),
            temperature: self.temperature,
            image_path: self.image_path.clone(),
        })
    }
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("okno"))
}

fn cache_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("weather_cache.json")
}

fn originals_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("original_wallpaper.json")
}

/// Убирает префикс \\?\ — Explorer его не любит.
fn plain_path(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(s) => PathBuf::from(s),
        None => p,
    }
}

pub fn bundled_scenes(app: &AppHandle) -> Option<PathBuf> {
    let dir = app
        .path()
        .resource_dir()
        .ok()?
        .join("assets")
        .join("scenes");
    Some(plain_path(dir))
}

fn local_hhmm(unix: i64, offset: i64) -> String {
    let m = (unix + offset).rem_euclid(86400) / 60;
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Считает состояние без побочных эффектов.
pub fn compute(app: &AppHandle, inner: &Inner, now: i64) -> SceneState {
    let s = &inner.settings;
    let mut st = SceneState {
        city: s.city.clone(),
        scene: None,
        phase: None,
        weather: None,
        sun_elevation: None,
        cloud_cover: None,
        temperature: inner.weather.as_ref().map(|w| w.temperature),
        sunrise: None,
        sunset: None,
        updated_at: None,
        mode: s.mode,
        custom_folder: s.custom_folder.clone(),
        preview: inner.preview.map(Scene::key),
        paused: inner.tray_paused || inner.auto_pause.is_some(),
        pause_reason: if inner.tray_paused {
            Some("tray".into())
        } else {
            inner.auto_pause.map(String::from)
        },
        offline: inner.offline,
        warning: inner.warning.clone(),
        image_path: None,
    };

    let real = s.city.as_ref().map(|c| {
        let offset = inner
            .weather
            .as_ref()
            .map(|w| w.utc_offset)
            .unwrap_or_else(|| (c.longitude / 15.0).round() as i64 * 3600);
        let (rise, set) = sun::sunrise_sunset(c.latitude, c.longitude, now, offset);
        st.sunrise = rise.map(|t| local_hhmm(t, offset));
        st.sunset = set.map(|t| local_hhmm(t, offset));
        st.updated_at = inner
            .weather
            .as_ref()
            .map(|w| local_hhmm(w.fetched_at, offset));
        let scene = Scene {
            phase: sun::phase(c.latitude, c.longitude, now),
            weather: inner
                .weather
                .as_ref()
                .map_or(WeatherKind::Clear, Weather::kind),
        };
        (scene, sun::elevation(c.latitude, c.longitude, now))
    });

    let shown = match (inner.preview, real) {
        (Some(p), _) => Some((
            p,
            p.phase.typical_elevation(),
            p.weather.typical_cloud_cover(),
        )),
        (None, Some((scene, elev))) => {
            let cc = inner
                .weather
                .as_ref()
                .map_or(scene.weather.typical_cloud_cover(), |w| w.cloud_cover);
            Some((scene, elev, cc))
        }
        (None, None) => None,
    };
    if let Some((scene, elev, cc)) = shown {
        st.scene = Some(scene.key());
        st.phase = Some(scene.phase);
        st.weather = Some(scene.weather);
        st.sun_elevation = Some(elev);
        st.cloud_cover = Some(cc);
        let custom = s.custom_folder.as_deref().map(Path::new);
        let bundled = bundled_scenes(app);
        let folders: Vec<&Path> = custom.into_iter().chain(bundled.as_deref()).collect();
        st.image_path = scene
            .find_image(&folders)
            .map(|p| plain_path(p).to_string_lossy().into_owned());
    }
    st
}

pub fn state(app: &AppHandle) -> SceneState {
    let st = app.state::<AppState>();
    let inner = st.lock();
    compute(app, &inner, chrono::Utc::now().timestamp())
}

// ---- Настройки ----

pub fn load_settings(app: &AppHandle) -> Settings {
    app.store(STORE)
        .ok()
        .and_then(|s| s.get("settings"))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

pub fn save_settings(app: &AppHandle) {
    let settings = app.state::<AppState>().lock().settings.clone();
    if let Ok(store) = app.store(STORE) {
        store.set(
            "settings",
            serde_json::to_value(settings).unwrap_or_default(),
        );
        let _ = store.save();
    }
}

pub fn set_city(app: &AppHandle, city: City) {
    let cached = weather::load_cache(&cache_path(app), city.latitude, city.longitude);
    {
        let st = app.state::<AppState>();
        let mut inner = st.lock();
        inner.settings.city = Some(city);
        inner.weather = cached;
        inner.offline = false;
        inner.next_fetch = 0;
    }
    save_settings(app);
}

// ---- Обновление ----

/// Один шаг: при необходимости погода, затем сцена и обои.
pub async fn tick(app: &AppHandle, force_weather: bool) -> SceneState {
    let st = app.state::<AppState>();
    let _guard = st.tick_lock.lock().await;
    let now = chrono::Utc::now().timestamp();

    let need = {
        let inner = st.lock();
        inner
            .settings
            .city
            .clone()
            .filter(|_| force_weather || now >= inner.next_fetch)
    };
    if let Some(city) = need {
        match weather::fetch(&st.client, city.latitude, city.longitude).await {
            Ok(w) => {
                weather::save_cache(&cache_path(app), city.latitude, city.longitude, &w);
                let mut inner = st.lock();
                inner.weather = Some(w);
                inner.offline = false;
                inner.next_fetch = now + WEATHER_PERIOD;
            }
            Err(message) => {
                let mut inner = st.lock();
                inner.offline = true;
                inner.next_fetch = now + WEATHER_RETRY;
                drop(inner);
                let _ = app.emit("weather-error", serde_json::json!({ "message": message }));
            }
        }
    }

    // Живое окно могло пропасть (например, перезапуск Explorer) — пересоздаём.
    let mode = st.lock().settings.mode;
    if mode == Mode::Live && !live_mode::is_running(app) {
        ensure_live(app).await;
    }

    let (state, paused, applied) = {
        let inner = st.lock();
        (
            compute(app, &inner, now),
            inner.tray_paused,
            inner.applied.clone(),
        )
    };

    if !paused {
        if let Some(img) = state.image_path.as_ref().map(PathBuf::from) {
            if applied.as_ref() != Some(&img) {
                apply_static(app, img).await;
            }
        }
    }
    if let Some(u) = state.update() {
        let _ = app.emit("scene-update", u);
    }
    let _ = app.emit("state-changed", &state);
    state
}

async fn apply_static(app: &AppHandle, img: PathBuf) {
    let st = app.state::<AppState>();
    if st.lock().originals.is_none() {
        let path = originals_path(app);
        let saved = std::fs::read(&path)
            .ok()
            .and_then(|d| serde_json::from_slice::<static_mode::Originals>(&d).ok());
        // Файл остался с прошлого запуска — значит, выход был аварийным, берём его.
        let originals = match saved {
            Some(o) => Some(o),
            None => {
                let cur = tauri::async_runtime::spawn_blocking(static_mode::current)
                    .await
                    .ok()
                    .and_then(Result::ok);
                if let Some(o) = &cur {
                    let _ = std::fs::create_dir_all(data_dir(app));
                    let _ = std::fs::write(&path, serde_json::to_vec(o).unwrap_or_default());
                }
                cur
            }
        };
        st.lock().originals = originals;
    }
    let target = img.clone();
    let res = tauri::async_runtime::spawn_blocking(move || static_mode::set(&target))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    match res {
        Ok(()) => st.lock().applied = Some(img),
        Err(e) => eprintln!("обои: {e}"),
    }
}

/// Возвращает исходные обои пользователя (вызывается при выходе).
pub fn restore(app: &AppHandle) {
    live_mode::stop(app);
    let st = app.state::<AppState>();
    let originals = {
        let mut inner = st.lock();
        inner.applied = None;
        inner.originals.take()
    };
    if let Some(o) = originals {
        if static_mode::restore(&o).is_ok() {
            let _ = std::fs::remove_file(originals_path(app));
        }
    }
}

/// Включает живой режим; при неудаче — статичный с предупреждением.
pub async fn ensure_live(app: &AppHandle) {
    if let Err(e) = live_mode::start(app).await {
        let st = app.state::<AppState>();
        {
            let mut inner = st.lock();
            inner.settings.mode = Mode::Static;
            inner.warning = Some(format!(
                "Живые обои не удалось разместить под иконками, включён статичный режим. {e}"
            ));
        }
        save_settings(app);
    }
}

pub async fn set_mode(app: &AppHandle, mode: Mode) -> SceneState {
    {
        let st = app.state::<AppState>();
        let mut inner = st.lock();
        inner.settings.mode = mode;
        inner.warning = None;
    }
    save_settings(app);
    match mode {
        Mode::Live => ensure_live(app).await,
        Mode::Static => live_mode::stop(app),
    }
    tick(app, false).await
}

pub fn set_tray_paused(app: &AppHandle, paused: bool) {
    app.state::<AppState>().lock().tray_paused = paused;
    emit_pause(app);
}

fn emit_pause(app: &AppHandle) {
    let (paused, reason) = {
        let st = app.state::<AppState>();
        let inner = st.lock();
        let reason = if inner.tray_paused {
            Some("tray")
        } else {
            inner.auto_pause
        };
        (reason.is_some(), reason)
    };
    let _ = app.emit(
        "live-pause",
        serde_json::json!({ "paused": paused, "reason": reason }),
    );
}

/// Фоновые циклы: пересчёт раз в минуту и проверка паузы раз в секунду.
pub fn spawn_loops(app: &AppHandle) {
    let a = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tick(&a, false).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
    let a = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let reason = if live_mode::is_running(&a) {
                tauri::async_runtime::spawn_blocking(pause::auto_reason)
                    .await
                    .unwrap_or(None)
            } else {
                None
            };
            let changed = {
                let st = a.state::<AppState>();
                let mut inner = st.lock();
                let changed = inner.auto_pause != reason;
                inner.auto_pause = reason;
                changed
            };
            if changed {
                emit_pause(&a);
            }
        }
    });
}
