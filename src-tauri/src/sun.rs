//! Положение солнца: высота над горизонтом, восход/закат, фаза суток.
//! Без внешних зависимостей, точность ~0.1°, для обоев этого достаточно.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Night,
    Dawn,
    Day,
    Sunset,
}

impl Phase {
    pub const ALL: [Phase; 4] = [Phase::Night, Phase::Dawn, Phase::Day, Phase::Sunset];

    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Night => "night",
            Phase::Dawn => "dawn",
            Phase::Day => "day",
            Phase::Sunset => "sunset",
        }
    }

    pub fn parse(s: &str) -> Option<Phase> {
        Phase::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// Условная высота солнца для ручного превью сцены.
    pub fn typical_elevation(self) -> f64 {
        match self {
            Phase::Night => -20.0,
            Phase::Dawn => 0.0,
            Phase::Day => 35.0,
            Phase::Sunset => 0.0,
        }
    }
}

/// Высота солнца в градусах для момента `unix` (секунды UTC).
pub fn elevation(lat: f64, lon: f64, unix: i64) -> f64 {
    let n = unix as f64 / 86400.0 + 2440587.5 - 2451545.0;
    let l = (280.460 + 0.985_647_4 * n).rem_euclid(360.0);
    let g = (357.528 + 0.985_600_3 * n).rem_euclid(360.0).to_radians();
    let lambda = (l + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();
    let eps = (23.439 - 0.000_000_4 * n).to_radians();
    let ra = (eps.cos() * lambda.sin()).atan2(lambda.cos()).to_degrees();
    let dec = (eps.sin() * lambda.sin()).asin();
    let gmst = (18.697_374_558 + 24.065_709_824_419_08 * n).rem_euclid(24.0);
    let ha = (gmst * 15.0 + lon - ra).to_radians();
    let lat = lat.to_radians();
    (lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos())
        .asin()
        .to_degrees()
}

/// Фаза суток по высоте солнца и направлению его движения.
pub fn phase(lat: f64, lon: f64, unix: i64) -> Phase {
    let h = elevation(lat, lon, unix);
    if h < -6.0 {
        Phase::Night
    } else if h < 6.0 {
        if elevation(lat, lon, unix + 600) > h {
            Phase::Dawn
        } else {
            Phase::Sunset
        }
    } else {
        Phase::Day
    }
}

/// Восход и закат (unix-время) в местные сутки, содержащие `unix`.
/// `utc_offset` — смещение часового пояса города в секундах.
/// `None` — полярный день или ночь.
pub fn sunrise_sunset(
    lat: f64,
    lon: f64,
    unix: i64,
    utc_offset: i64,
) -> (Option<i64>, Option<i64>) {
    const HORIZON: f64 = -0.833;
    let day_start = (unix + utc_offset).div_euclid(86400) * 86400 - utc_offset;
    let (mut rise, mut set) = (None, None);
    let mut prev = elevation(lat, lon, day_start);
    for m in 1..=1440 {
        let t = day_start + m * 60;
        let h = elevation(lat, lon, t);
        if prev < HORIZON && h >= HORIZON && rise.is_none() {
            rise = Some(t);
        }
        if prev >= HORIZON && h < HORIZON && set.is_none() {
            set = Some(t);
        }
        prev = h;
    }
    (rise, set)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Москва, 2024-06-21 12:30 MSK (09:30 UTC): солнце высоко, около 57°.
    #[test]
    fn moscow_summer_noon() {
        let h = elevation(55.75, 37.62, 1_718_962_200);
        assert!((h - 57.5).abs() < 1.0, "h = {h}");
    }

    #[test]
    fn moscow_sunrise_sunset() {
        // 2024-06-21: восход ~03:44, закат ~21:18 по Москве.
        let (r, s) = sunrise_sunset(55.75, 37.62, 1_718_962_200, 3 * 3600);
        let r = (r.unwrap() + 3 * 3600).rem_euclid(86400) / 60;
        let s = (s.unwrap() + 3 * 3600).rem_euclid(86400) / 60;
        assert!((r - (3 * 60 + 44)).abs() <= 4, "rise {r}");
        assert!((s - (21 * 60 + 18)).abs() <= 4, "set {s}");
    }

    #[test]
    fn phases() {
        // 2024-06-21 00:00 MSK — ночь, 03:45 — рассвет, 21:15 — закат.
        assert_eq!(phase(55.75, 37.62, 1_718_917_200), Phase::Night);
        assert_eq!(phase(55.75, 37.62, 1_718_930_700), Phase::Dawn);
        assert_eq!(phase(55.75, 37.62, 1_718_993_700), Phase::Sunset);
        assert_eq!(phase(55.75, 37.62, 1_718_962_200), Phase::Day);
    }

    #[test]
    fn polar_night() {
        // Мурманск, 2024-12-21: солнце не встаёт.
        let (r, s) = sunrise_sunset(68.97, 33.08, 1_734_775_200, 3 * 3600);
        assert!(r.is_none() && s.is_none());
    }
}
