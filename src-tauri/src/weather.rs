//! Погода Open-Meteo, поиск города, автоопределение по IP, кэш на диске.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WeatherKind {
    Clear,
    Cloudy,
    Fog,
    Rain,
    Snow,
    Storm,
}

impl WeatherKind {
    pub const ALL: [WeatherKind; 6] = [
        WeatherKind::Clear,
        WeatherKind::Cloudy,
        WeatherKind::Fog,
        WeatherKind::Rain,
        WeatherKind::Snow,
        WeatherKind::Storm,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            WeatherKind::Clear => "clear",
            WeatherKind::Cloudy => "cloudy",
            WeatherKind::Fog => "fog",
            WeatherKind::Rain => "rain",
            WeatherKind::Snow => "snow",
            WeatherKind::Storm => "storm",
        }
    }

    pub fn parse(s: &str) -> Option<WeatherKind> {
        WeatherKind::ALL.into_iter().find(|w| w.as_str() == s)
    }

    /// Код WMO → тип погоды. Неизвестные коды считаем облачностью.
    pub fn from_wmo(code: i32) -> WeatherKind {
        match code {
            0 | 1 => WeatherKind::Clear,
            2 | 3 => WeatherKind::Cloudy,
            45 | 48 => WeatherKind::Fog,
            51..=67 | 80..=82 => WeatherKind::Rain,
            71..=77 | 85 | 86 => WeatherKind::Snow,
            95..=99 => WeatherKind::Storm,
            _ => WeatherKind::Cloudy,
        }
    }

    /// Условная облачность для ручного превью сцены.
    pub fn typical_cloud_cover(self) -> f64 {
        match self {
            WeatherKind::Clear => 5.0,
            WeatherKind::Cloudy => 75.0,
            WeatherKind::Fog => 90.0,
            WeatherKind::Rain | WeatherKind::Snow => 90.0,
            WeatherKind::Storm => 100.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct City {
    pub name: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub admin1: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weather {
    pub temperature: f64,
    pub weather_code: i32,
    pub cloud_cover: f64,
    pub is_day: bool,
    /// Смещение часового пояса города, секунды.
    pub utc_offset: i64,
    /// Когда получен ответ, unix-секунды.
    pub fetched_at: i64,
}

impl Weather {
    pub fn kind(&self) -> WeatherKind {
        WeatherKind::from_wmo(self.weather_code)
    }
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("Okno/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
}

#[derive(Deserialize)]
struct ForecastResponse {
    #[serde(default)]
    utc_offset_seconds: i64,
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f64,
    weather_code: i32,
    cloud_cover: f64,
    is_day: u8,
}

pub async fn fetch(client: &reqwest::Client, lat: f64, lon: f64) -> Result<Weather, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,weather_code,cloud_cover,is_day&timezone=auto"
    );
    let r: ForecastResponse = get_json(client, &url).await?;
    Ok(Weather {
        temperature: r.current.temperature_2m,
        weather_code: r.current.weather_code,
        cloud_cover: r.current.cloud_cover,
        is_day: r.current.is_day == 1,
        utc_offset: r.utc_offset_seconds,
        fetched_at: chrono::Utc::now().timestamp(),
    })
}

#[derive(Deserialize)]
struct GeoResponse {
    #[serde(default)]
    results: Vec<City>,
}

pub async fn search_city(client: &reqwest::Client, query: &str) -> Result<Vec<City>, String> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let url = reqwest::Url::parse_with_params(
        "https://geocoding-api.open-meteo.com/v1/search",
        &[("name", q), ("language", "ru"), ("count", "5")],
    )
    .map_err(|e| e.to_string())?;
    let r: GeoResponse = get_json(client, url.as_str()).await?;
    Ok(r.results)
}

#[derive(Deserialize)]
struct IpWhoIs {
    success: bool,
    city: Option<String>,
    country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Deserialize)]
struct IpApiCo {
    city: Option<String>,
    country_name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

/// Город по IP: ipwho.is, затем ipapi.co. Название уточняется геокодером на русском.
pub async fn detect_city(client: &reqwest::Client) -> Result<City, String> {
    let ip_city = match get_json::<IpWhoIs>(client, "https://ipwho.is/").await {
        Ok(r) if r.success => to_city(r.city, r.country, r.latitude, r.longitude),
        _ => None,
    };
    let ip_city = match ip_city {
        Some(c) => c,
        None => {
            let r: IpApiCo = get_json(client, "https://ipapi.co/json/")
                .await
                .map_err(|_| "Не удалось определить город по IP".to_string())?;
            to_city(r.city, r.country_name, r.latitude, r.longitude)
                .ok_or("Не удалось определить город по IP")?
        }
    };
    // Русское название и часовой пояс: ближайший результат геокодера в пределах ~50 км.
    if let Ok(found) = search_city(client, &ip_city.name).await {
        if let Some(c) = found.into_iter().find(|c| {
            (c.latitude - ip_city.latitude).abs() < 0.5
                && (c.longitude - ip_city.longitude).abs() < 0.8
        }) {
            return Ok(c);
        }
    }
    Ok(ip_city)
}

fn to_city(
    name: Option<String>,
    country: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Option<City> {
    Some(City {
        name: name.filter(|n| !n.is_empty())?,
        country,
        admin1: None,
        latitude: lat?,
        longitude: lon?,
        timezone: None,
    })
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Нет связи: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Сервер ответил {}", resp.status()));
    }
    resp.json::<T>()
        .await
        .map_err(|e| format!("Неверный ответ сервера: {e}"))
}

// ---- Кэш последнего ответа ----

#[derive(Serialize, Deserialize)]
struct CacheFile {
    latitude: f64,
    longitude: f64,
    weather: Weather,
}

pub fn load_cache(path: &Path, lat: f64, lon: f64) -> Option<Weather> {
    let data = std::fs::read(path).ok()?;
    let c: CacheFile = serde_json::from_slice(&data).ok()?;
    ((c.latitude - lat).abs() < 0.01 && (c.longitude - lon).abs() < 0.01).then_some(c.weather)
}

pub fn save_cache(path: &Path, lat: f64, lon: f64, weather: &Weather) {
    let c = CacheFile {
        latitude: lat,
        longitude: lon,
        weather: weather.clone(),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(data) = serde_json::to_vec(&c) {
        let _ = std::fs::write(path, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wmo_codes() {
        assert_eq!(WeatherKind::from_wmo(0), WeatherKind::Clear);
        assert_eq!(WeatherKind::from_wmo(3), WeatherKind::Cloudy);
        assert_eq!(WeatherKind::from_wmo(48), WeatherKind::Fog);
        assert_eq!(WeatherKind::from_wmo(61), WeatherKind::Rain);
        assert_eq!(WeatherKind::from_wmo(81), WeatherKind::Rain);
        assert_eq!(WeatherKind::from_wmo(75), WeatherKind::Snow);
        assert_eq!(WeatherKind::from_wmo(86), WeatherKind::Snow);
        assert_eq!(WeatherKind::from_wmo(96), WeatherKind::Storm);
    }
}
