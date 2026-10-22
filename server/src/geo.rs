//! Great-circle helpers (nautical miles, true degrees).

const R_NM: f64 = 3440.065;

pub fn distance_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R_NM * a.sqrt().asin()
}

pub fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Point a fraction `f` of the way along the great circle.
pub fn interpolate(lat1: f64, lon1: f64, lat2: f64, lon2: f64, f: f64) -> (f64, f64) {
    let d = distance_nm(lat1, lon1, lat2, lon2) / R_NM;
    if d < 1e-9 {
        return (lat1, lon1);
    }
    let (p1, l1, p2, l2) = (lat1.to_radians(), lon1.to_radians(), lat2.to_radians(), lon2.to_radians());
    let a = ((1.0 - f) * d).sin() / d.sin();
    let b = (f * d).sin() / d.sin();
    let x = a * p1.cos() * l1.cos() + b * p2.cos() * l2.cos();
    let y = a * p1.cos() * l1.sin() + b * p2.cos() * l2.sin();
    let z = a * p1.sin() + b * p2.sin();
    (z.atan2((x * x + y * y).sqrt()).to_degrees(), y.atan2(x).to_degrees())
}

/// Destination from a start point, true bearing and distance.
pub fn offset(lat: f64, lon: f64, brg: f64, dist_nm: f64) -> (f64, f64) {
    let d = dist_nm / R_NM;
    let (p1, l1, t) = (lat.to_radians(), lon.to_radians(), brg.to_radians());
    let p2 = (p1.sin() * d.cos() + p1.cos() * d.sin() * t.cos()).asin();
    let l2 = l1 + (t.sin() * d.sin() * p1.cos()).atan2(d.cos() - p1.sin() * p2.sin());
    (p2.to_degrees(), l2.to_degrees())
}

/// Cross-track distance (NM) of a point from the segment start->end, and along-track distance.
pub fn cross_along(lat1: f64, lon1: f64, lat2: f64, lon2: f64, lat: f64, lon: f64) -> (f64, f64) {
    let d13 = distance_nm(lat1, lon1, lat, lon) / R_NM;
    let t13 = bearing(lat1, lon1, lat, lon).to_radians();
    let t12 = bearing(lat1, lon1, lat2, lon2).to_radians();
    let xt = (d13.sin() * (t13 - t12).sin()).asin();
    let at = (d13.cos() / xt.cos()).clamp(-1.0, 1.0).acos();
    (xt * R_NM, at * R_NM)
}
