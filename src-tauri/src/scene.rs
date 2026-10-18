//! Сцена = фаза суток + погода, поиск картинки с запасными вариантами.

use crate::sun::Phase;
use crate::weather::WeatherKind;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scene {
    pub phase: Phase,
    pub weather: WeatherKind,
}

const EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

impl Scene {
    pub fn key(self) -> String {
        format!("{}_{}", self.phase.as_str(), self.weather.as_str())
    }

    pub fn parse(key: &str) -> Option<Scene> {
        let (p, w) = key.split_once('_')?;
        Some(Scene {
            phase: Phase::parse(p)?,
            weather: WeatherKind::parse(w)?,
        })
    }

    #[cfg(test)]
    pub fn all() -> impl Iterator<Item = Scene> {
        Phase::ALL.into_iter().flat_map(|phase| {
            WeatherKind::ALL
                .into_iter()
                .map(move |weather| Scene { phase, weather })
        })
    }

    /// Цепочка запасных вариантов: storm → rain → cloudy → clear и т.п.
    pub fn fallbacks(self) -> Vec<Scene> {
        use WeatherKind::*;
        let chain: &[WeatherKind] = match self.weather {
            Storm => &[Storm, Rain, Cloudy, Clear],
            Rain => &[Rain, Cloudy, Clear],
            Snow => &[Snow, Cloudy, Clear],
            Fog => &[Fog, Cloudy, Clear],
            Cloudy => &[Cloudy, Clear],
            Clear => &[Clear],
        };
        chain
            .iter()
            .map(|&weather| Scene {
                phase: self.phase,
                weather,
            })
            .collect()
    }

    /// Первая найденная картинка по цепочке. Для каждого варианта сначала
    /// своя папка пользователя, потом встроенные картинки.
    pub fn find_image(self, folders: &[&Path]) -> Option<PathBuf> {
        self.fallbacks().into_iter().find_map(|s| {
            let key = s.key();
            folders.iter().find_map(|dir| {
                EXTENSIONS
                    .iter()
                    .map(|ext| dir.join(format!("{key}.{ext}")))
                    .find(|p| p.is_file())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_roundtrip() {
        assert_eq!(Scene::all().count(), 24);
        for s in Scene::all() {
            assert_eq!(Scene::parse(&s.key()), Some(s));
        }
        assert_eq!(Scene::parse("noon_rain"), None);
    }

    #[test]
    fn storm_chain() {
        let s = Scene::parse("sunset_storm").unwrap();
        let keys: Vec<_> = s.fallbacks().into_iter().map(Scene::key).collect();
        assert_eq!(
            keys,
            [
                "sunset_storm",
                "sunset_rain",
                "sunset_cloudy",
                "sunset_clear"
            ]
        );
    }

    #[test]
    fn find_with_fallback() {
        let dir = std::env::temp_dir().join("okno_scene_test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("night_cloudy.jpg"), b"x").unwrap();
        let found = Scene::parse("night_storm")
            .unwrap()
            .find_image(&[&dir])
            .unwrap();
        assert!(found.ends_with("night_cloudy.jpg"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
