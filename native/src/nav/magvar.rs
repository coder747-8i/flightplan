//! Magnetic variation from the World Magnetic Model (WMM-2025), east positive.
//! Results are cached per quarter-degree cell and day: the navlog asks for many nearby points.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;
use world_magnetic_model::{
    time::Date,
    uom::si::{angle::degree, f32::{Angle, Length}, length::meter},
    GeomagneticField,
};

static CACHE: LazyLock<Mutex<HashMap<(i32, i32, i32), f64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn variation(lat: f64, lon: f64, when: chrono::DateTime<chrono::Utc>) -> f64 {
    use chrono::Datelike;
    let day = when.ordinal() as i32 + when.year() * 1000;
    let key = ((lat * 4.0).round() as i32, (lon * 4.0).round() as i32, day);
    if let Some(v) = CACHE.lock().get(&key) {
        return *v;
    }
    let date = Date::from_ordinal_date(when.year(), when.ordinal() as u16).ok();
    let v = date
        .and_then(|d| GeomagneticField::new(Length::new::<meter>(0.0), Angle::new::<degree>(lat as f32), Angle::new::<degree>(lon as f32), d).ok())
        .map(|f| f.declination().get::<degree>() as f64)
        .unwrap_or(0.0);
    let mut c = CACHE.lock();
    if c.len() > 50_000 {
        c.clear();
    }
    c.insert(key, v);
    v
}

/// Variation now (for chart symbology such as compass roses).
pub fn variation_now(lat: f64, lon: f64) -> f64 {
    variation(lat, lon, chrono::Utc::now())
}

#[cfg(test)]
mod tests {
    #[test]
    fn phoenix_is_east() {
        let v = super::variation_now(33.43, -112.01);
        assert!(v > 8.0 && v < 12.0, "{v}");
    }
}
