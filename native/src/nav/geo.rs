//! Great-circle helpers (NM, degrees true).

const R: f64 = 3440.065;

pub fn distance_nm(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (dl / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

pub fn bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let y = (lon2 - lon1).to_radians().sin() * lat2.to_radians().cos();
    let x = lat1.to_radians().cos() * lat2.to_radians().sin() - lat1.to_radians().sin() * lat2.to_radians().cos() * (lon2 - lon1).to_radians().cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Point a fraction `f` of the way along the great circle; returns (lat, lon).
pub fn interpolate(lat1: f64, lon1: f64, lat2: f64, lon2: f64, f: f64) -> (f64, f64) {
    let d = distance_nm(lat1, lon1, lat2, lon2) / R;
    if d < 1e-9 {
        return (lat1, lon1);
    }
    let a = ((1.0 - f) * d).sin() / d.sin();
    let b = (f * d).sin() / d.sin();
    let (p1, l1, p2, l2) = (lat1.to_radians(), lon1.to_radians(), lat2.to_radians(), lon2.to_radians());
    let x = a * p1.cos() * l1.cos() + b * p2.cos() * l2.cos();
    let y = a * p1.cos() * l1.sin() + b * p2.cos() * l2.sin();
    let z = a * p1.sin() + b * p2.sin();
    (z.atan2(x.hypot(y)).to_degrees(), y.atan2(x).to_degrees())
}

/// Densify a [lon, lat] path into great-circle segments for drawing.
pub fn great_circle(path: &[[f64; 2]], step_nm: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for i in 1..path.len() {
        let [lon1, lat1] = path[i - 1];
        let [lon2, lat2] = path[i];
        let n = ((distance_nm(lat1, lon1, lat2, lon2) / step_nm).ceil() as usize).max(1);
        for k in if i == 1 { 0 } else { 1 }..=n {
            let (la, lo) = interpolate(lat1, lon1, lat2, lon2, k as f64 / n as f64);
            out.push([lo, la]);
        }
    }
    out
}

/// Point at along-track distance `d` (NM) on a [lon, lat] waypoint path.
pub fn point_along(path: &[[f64; 2]], d: f64) -> [f64; 2] {
    let mut acc = 0.0;
    for i in 1..path.len() {
        let [lon1, lat1] = path[i - 1];
        let [lon2, lat2] = path[i];
        let len = distance_nm(lat1, lon1, lat2, lon2);
        if acc + len >= d {
            let (la, lo) = interpolate(lat1, lon1, lat2, lon2, (d - acc) / len.max(1e-6));
            return [lo, la];
        }
        acc += len;
    }
    *path.last().unwrap_or(&[0.0, 0.0])
}

/// Ray-casting point-in-ring test on [x, y] pairs.
pub fn in_ring(x: f64, y: f64, ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let [xi, yi] = ring[i];
        let [xj, yj] = ring[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BBox {
    pub fn of(pts: impl IntoIterator<Item = [f64; 2]>) -> Self {
        let mut b = BBox { min_x: f64::INFINITY, min_y: f64::INFINITY, max_x: f64::NEG_INFINITY, max_y: f64::NEG_INFINITY };
        for [x, y] in pts {
            b.min_x = b.min_x.min(x);
            b.min_y = b.min_y.min(y);
            b.max_x = b.max_x.max(x);
            b.max_y = b.max_y.max(y);
        }
        b
    }
    pub fn overlaps(&self, o: &BBox) -> bool {
        self.min_x <= o.max_x && self.max_x >= o.min_x && self.min_y <= o.max_y && self.max_y >= o.min_y
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

/// ICAO / FAA lat-long waypoint, e.g. 3330N11203W (whole minutes, carried into degrees).
pub fn coord_id(lat: f64, lon: f64) -> String {
    let part = |v: f64, width: usize| {
        let mut d = v.abs().floor() as i64;
        let mut m = ((v.abs() - d as f64) * 60.0).round() as i64;
        if m == 60 {
            d += 1;
            m = 0;
        }
        format!("{:0width$}{:02}", d, m, width = width)
    };
    format!("{}{}{}{}", part(lat, 2), if lat >= 0.0 { 'N' } else { 'S' }, part(lon, 3), if lon >= 0.0 { 'E' } else { 'W' })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_bearing() {
        // KPHX -> KSDL is about 10 NM
        let d = distance_nm(33.4343, -112.0116, 33.6229, -111.9105);
        assert!((d - 12.2).abs() < 0.5, "{d}");
        let b = bearing(0.0, 0.0, 1.0, 0.0);
        assert!(b.abs() < 1e-6);
    }

    #[test]
    fn coords() {
        assert_eq!(coord_id(33.9999, -112.5), "3400N11230W");
        assert_eq!(coord_id(34.5, -112.5), "3430N11230W");
    }
}
