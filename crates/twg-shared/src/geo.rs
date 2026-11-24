use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

pub const EARTH_RADIUS_M: f64 = 6_378_137.0;
pub const MAX_MERCATOR_LAT: f64 = 85.051_128_78;
pub const TILE_SIZE_PX: f64 = 256.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoAnchor {
    pub center_lng: f64,
    pub center_lat: f64,
    pub origin_mx: f64,
    pub origin_my: f64,
    pub cos_lat: f64,
}

impl Default for GeoAnchor {
    fn default() -> Self {
        Self::new(115.2167, -8.6705)
    }
}

impl GeoAnchor {
    pub fn new(center_lng: f64, center_lat: f64) -> Self {
        let clamped_lat = center_lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT);
        let (origin_mx, origin_my) = lng_lat_to_mercator(center_lng, clamped_lat);
        let cos_lat = clamped_lat.to_radians().cos().max(0.1);
        Self {
            center_lng,
            center_lat: clamped_lat,
            origin_mx,
            origin_my,
            cos_lat,
        }
    }

    /// Converts `(lng, lat, alt_m)` to Bevy local 3D meters around the anchor.
    /// Coordinate frame: `+X = East`, `+Y = Up`, `-Z = North`.
    pub fn lng_lat_alt_to_local(&self, lng: f64, lat: f64, alt_m: f64) -> [f32; 3] {
        let (mx, my) = lng_lat_to_mercator(lng, lat);
        let x = ((mx - self.origin_mx) * self.cos_lat) as f32;
        let y = alt_m as f32;
        let z = -((my - self.origin_my) * self.cos_lat) as f32;
        [x, y, z]
    }

    /// Converts Bevy local 3D meters (`[x, y, z]`) back to `(lng, lat, alt_m)`.
    pub fn local_to_lng_lat_alt(&self, pos: [f32; 3]) -> (f64, f64, f64) {
        let mx = self.origin_mx + (pos[0] as f64) / self.cos_lat;
        let my = self.origin_my - (pos[2] as f64) / self.cos_lat;
        let (lng, lat) = mercator_to_lng_lat(mx, my);
        (lng, lat, pos[1] as f64)
    }

    /// Returns `(center_local_xyz, width_x_m, height_z_m)` in local ground meters for a slippy tile.
    pub fn tile_local_rect(&self, tile: TileCoord) -> ([f32; 3], f32, f32) {
        let (min_mx, min_my, max_mx, max_my) = tile_bounds_mercator(tile);
        let cx = ((0.5 * (min_mx + max_mx) - self.origin_mx) * self.cos_lat) as f32;
        let cz = -((0.5 * (min_my + max_my) - self.origin_my) * self.cos_lat) as f32;
        let width_m = ((max_mx - min_mx) * self.cos_lat) as f32;
        let height_m = ((max_my - min_my) * self.cos_lat) as f32;
        ([cx, 0.0, cz], width_m, height_m)
    }
}

pub fn lng_lat_to_mercator(lng: f64, lat: f64) -> (f64, f64) {
    let clamped_lat = lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT);
    let mx = EARTH_RADIUS_M * lng.to_radians();
    let my = EARTH_RADIUS_M * (FRAC_PI_4 + clamped_lat.to_radians() * 0.5).tan().ln();
    (mx, my)
}

pub fn mercator_to_lng_lat(mx: f64, my: f64) -> (f64, f64) {
    let lng = (mx / EARTH_RADIUS_M).to_degrees();
    let lat = (2.0 * (my / EARTH_RADIUS_M).exp().atan() - FRAC_PI_2).to_degrees();
    (lng, lat)
}

pub fn lng_lat_to_tile(lng: f64, lat: f64, z: u8) -> TileCoord {
    let n = (1u64 << z) as f64;
    let clamped_lat = lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT);
    let lat_rad = clamped_lat.to_radians();
    let norm_x = (lng + 180.0) / 360.0;
    let norm_y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / PI) / 2.0;

    let max_idx = (1u64 << z).saturating_sub(1);
    let x = ((norm_x * n).floor() as i64).clamp(0, max_idx as i64) as u32;
    let y = ((norm_y * n).floor() as i64).clamp(0, max_idx as i64) as u32;
    TileCoord { z, x, y }
}

/// Returns `(min_mx, min_my, max_mx, max_my)` in EPSG:3857 meters.
pub fn tile_bounds_mercator(tile: TileCoord) -> (f64, f64, f64, f64) {
    let world_half = PI * EARTH_RADIUS_M;
    let world_size = 2.0 * world_half;
    let n = (1u64 << tile.z) as f64;
    let tile_span = world_size / n;

    let min_mx = -world_half + (tile.x as f64) * tile_span;
    let max_mx = min_mx + tile_span;
    // Slippy tile y=0 is at the North edge (+world_half).
    let max_my = world_half - (tile.y as f64) * tile_span;
    let min_my = max_my - tile_span;
    (min_mx, min_my, max_mx, max_my)
}

/// Computes continuous Web Mercator zoom level from camera distance and FOV.
pub fn zoom_for_camera_distance(
    distance_m: f32,
    viewport_height_px: f32,
    fov_y_rad: f32,
    cos_lat: f64,
) -> f64 {
    let vh = (viewport_height_px as f64).max(120.0);
    let visible_height_m = 2.0 * (distance_m as f64).max(0.25) * (fov_y_rad as f64 * 0.5).tan();
    let ground_m_per_px = visible_height_m / vh;
    let mercator_m_per_px = (ground_m_per_px / cos_lat.max(0.1)).max(0.0001);
    let world_size = 2.0 * PI * EARTH_RADIUS_M;
    (world_size / (TILE_SIZE_PX * mercator_m_per_px))
        .log2()
        .clamp(1.0, 26.0)
}

/// Inverse helper: camera distance in meters that matches a Web Mercator zoom level.
pub fn camera_distance_for_zoom(
    zoom: f64,
    viewport_height_px: f32,
    fov_y_rad: f32,
    cos_lat: f64,
) -> f32 {
    let vh = (viewport_height_px as f64).max(120.0);
    let world_size = 2.0 * PI * EARTH_RADIUS_M;
    let mercator_m_per_px = world_size / (TILE_SIZE_PX * 2.0_f64.powf(zoom.clamp(1.0, 26.0)));
    let ground_m_per_px = mercator_m_per_px * cos_lat.max(0.1);
    let visible_height_m = ground_m_per_px * vh;
    let denom = (2.0 * (fov_y_rad as f64 * 0.5).tan()).max(0.05);
    (visible_height_m / denom).clamp(0.5, 100_000_000.0) as f32
}

pub fn osm_tile_url(tile: TileCoord) -> String {
    format!(
        "https://tile.openstreetmap.org/{}/{}/{}.png",
        tile.z, tile.x, tile.y
    )
}

/// NOTE: ArcGIS World Imagery expects `{z}/{y}/{x}` (row then column).
pub fn arcgis_satellite_tile_url(tile: TileCoord) -> String {
    format!(
        "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{}/{}/{}",
        tile.z, tile.y, tile.x
    )
}

pub fn haversine_distance_m(lng1: f64, lat1: f64, lng2: f64, lat2: f64) -> f64 {
    let r = 6_371_000.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();
    let a = (dlat * 0.5).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlng * 0.5).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

pub fn polyline_length_m(points: &[[f64; 2]]) -> f64 {
    points
        .windows(2)
        .map(|w| haversine_distance_m(w[0][0], w[0][1], w[1][0], w[1][1]))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mercator_and_anchor_roundtrip_submillimeter() {
        let anchor = GeoAnchor::new(115.2167, -8.6705);
        let test_lng = 115.221234;
        let test_lat = -8.668123;
        let local = anchor.lng_lat_alt_to_local(test_lng, test_lat, 12.5);
        let (lng_back, lat_back, alt_back) = anchor.local_to_lng_lat_alt(local);

        let err_m = haversine_distance_m(test_lng, test_lat, lng_back, lat_back);
        assert!(err_m < 0.01, "expected <1cm error, got {err_m}m");
        assert!((alt_back - 12.5).abs() < 1e-4);
    }

    #[test]
    fn tile_urls_have_correct_axis_order() {
        let tile = TileCoord {
            z: 16,
            x: 53744,
            y: 34347,
        };
        assert_eq!(
            osm_tile_url(tile),
            "https://tile.openstreetmap.org/16/53744/34347.png"
        );
        assert_eq!(
            arcgis_satellite_tile_url(tile),
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/16/34347/53744"
        );
    }

    #[test]
    fn tile_contains_origin_coord() {
        let lng = 115.2167;
        let lat = -8.6705;
        let tile = lng_lat_to_tile(lng, lat, 16);
        let (min_mx, min_my, max_mx, max_my) = tile_bounds_mercator(tile);
        let (mx, my) = lng_lat_to_mercator(lng, lat);
        assert!(mx >= min_mx && mx <= max_mx);
        assert!(my >= min_my && my <= max_my);
    }

    #[test]
    fn zoom_distance_roundtrip() {
        let anchor = GeoAnchor::default();
        for z in [3.0, 16.5, 22.0, 26.0] {
            let dist = camera_distance_for_zoom(z, 900.0, 45.0_f32.to_radians(), anchor.cos_lat);
            let zoom_back =
                zoom_for_camera_distance(dist, 900.0, 45.0_f32.to_radians(), anchor.cos_lat);
            assert!(
                (zoom_back - z).abs() < 0.02,
                "z={z} dist={dist} zoom_back={zoom_back}"
            );
        }
    }
}
