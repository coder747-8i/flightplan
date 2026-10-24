//! Sunrise / sunset / civil twilight (NOAA solar position approximation, ±1-2 min).

use chrono::{DateTime, TimeZone, Utc};

/// Times for the UTC day containing `when`; `zenith` 90.833 = sunrise/set, 96 = civil twilight.
pub fn sun_times(lat: f64, lon: f64, when: DateTime<Utc>, zenith: f64) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let day = Utc.from_utc_datetime(&when.date_naive().and_hms_opt(0, 0, 0)?);
    let julian = day.timestamp() as f64 / 86400.0 + 2440587.5;
    let n = (julian - 2451545.0 + 0.0008).round();
    let j_star = n - lon / 360.0;
    let m = (357.5291 + 0.98560028 * j_star) % 360.0;
    let c = 1.9148 * m.to_radians().sin() + 0.02 * (2.0 * m).to_radians().sin() + 0.0003 * (3.0 * m).to_radians().sin();
    let lambda = (m + c + 180.0 + 102.9372) % 360.0;
    let j_transit = 2451545.0 + j_star + 0.0053 * m.to_radians().sin() - 0.0069 * (2.0 * lambda).to_radians().sin();
    let decl = (lambda.to_radians().sin() * 23.4397f64.to_radians().sin()).asin();
    let cos_h = (zenith.to_radians().cos() - lat.to_radians().sin() * decl.sin()) / (lat.to_radians().cos() * decl.cos());
    if !(-1.0..=1.0).contains(&cos_h) {
        return None;
    }
    let h = cos_h.acos().to_degrees();
    let to_date = |j: f64| Utc.timestamp_millis_opt(((j - 2440587.5) * 86_400_000.0) as i64).single();
    Some((to_date(j_transit - h / 360.0)?, to_date(j_transit + h / 360.0)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phoenix_summer() {
        let (rise, set) = sun_times(33.43, -112.01, Utc.with_ymd_and_hms(2026, 6, 21, 18, 0, 0).unwrap(), 90.833).unwrap();
        // ~12:20Z rise and ~03:42Z (next UTC day) set
        assert_eq!(rise.format("%H").to_string(), "12");
        assert!((set - rise).num_minutes() > 14 * 60);
    }
}
