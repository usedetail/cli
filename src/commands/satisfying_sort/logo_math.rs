use std::sync::OnceLock;

use super::numeric::{floor_f32_to_usize, round_f32_to_usize, usize_to_f32};
use super::LOGO_MASK_SIZE;

const TOP_TRIANGLE_DIAG_MAX: f32 = 0.566;
const CENTER_BAND_A_MIN: f32 = 0.734;
const CENTER_BAND_A_MAX: f32 = 0.914;
const CENTER_BAND_B_MIN: f32 = 1.086;
const CENTER_BAND_B_MAX: f32 = 1.266;
const BOTTOM_TRIANGLE_DIAG_MIN: f32 = 1.434;
const TOP_TRIANGLE_X_SPAN: f32 = TOP_TRIANGLE_DIAG_MAX;
const BOTTOM_TRIANGLE_X_SPAN: f32 = TOP_TRIANGLE_DIAG_MAX;

static LOGO_MASK: OnceLock<LogoMask> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LogoRegion {
    Empty,
    Static,
    TopTriangle,
    BottomTriangle,
}

#[derive(Clone)]
struct LogoMask {
    width: usize,
    height: usize,
    regions: Vec<LogoRegion>,
}

fn generate_logo_mask(size: usize) -> LogoMask {
    let side = size.max(64);
    let mut regions = vec![LogoRegion::Empty; side * side];
    let denom = usize_to_f32(side.saturating_sub(1).max(1));

    for y in 0..side {
        for x in 0..side {
            let d = (usize_to_f32(x) + usize_to_f32(y)) / denom;
            regions[y * side + x] = if d <= TOP_TRIANGLE_DIAG_MAX {
                LogoRegion::TopTriangle
            } else if d >= BOTTOM_TRIANGLE_DIAG_MIN {
                LogoRegion::BottomTriangle
            } else if (CENTER_BAND_A_MIN..=CENTER_BAND_A_MAX).contains(&d)
                || (CENTER_BAND_B_MIN..=CENTER_BAND_B_MAX).contains(&d)
            {
                LogoRegion::Static
            } else {
                LogoRegion::Empty
            };
        }
    }

    LogoMask {
        width: side,
        height: side,
        regions,
    }
}

pub(super) fn triangle_nominal_index(nx: f32, n: usize, region: LogoRegion) -> usize {
    let n = n.max(1);
    let span = match region {
        LogoRegion::TopTriangle => TOP_TRIANGLE_X_SPAN,
        LogoRegion::BottomTriangle => BOTTOM_TRIANGLE_X_SPAN,
        LogoRegion::Empty | LogoRegion::Static => 1.0,
    };
    let direction_x = match region {
        LogoRegion::TopTriangle => nx,
        LogoRegion::BottomTriangle => 1.0 - nx,
        LogoRegion::Empty | LogoRegion::Static => 0.0,
    };
    let t = (direction_x / span).clamp(0.0, 1.0);
    floor_f32_to_usize(t * usize_to_f32(n)).min(n - 1)
}

pub(super) fn shuffled_index_for_nominal(array: &[usize], nominal_index: usize, n: usize) -> usize {
    let n = n.max(1);
    array
        .get(nominal_index.min(n - 1))
        .copied()
        .unwrap_or(nominal_index.saturating_add(1))
        .saturating_sub(1)
        .min(n - 1)
}

pub(super) fn source_local_x_for_index(
    shuffled_index: usize,
    n: usize,
    region: LogoRegion,
    viewport_width: usize,
) -> usize {
    if viewport_width == 0 {
        return 0;
    }
    let max_col = viewport_width - 1;
    let unit = if n <= 1 {
        0.0
    } else {
        usize_to_f32(shuffled_index.min(n - 1)) / usize_to_f32(n - 1)
    };

    let nx = match region {
        LogoRegion::TopTriangle => (unit * TOP_TRIANGLE_X_SPAN).clamp(0.0, 1.0),
        LogoRegion::BottomTriangle => (1.0 - unit * BOTTOM_TRIANGLE_X_SPAN).clamp(0.0, 1.0),
        LogoRegion::Static | LogoRegion::Empty => unit.clamp(0.0, 1.0),
    };
    round_f32_to_usize(nx * usize_to_f32(max_col)).min(max_col)
}

pub(super) fn logo_region_at(
    local_x: usize,
    local_y: usize,
    viewport_width: usize,
    viewport_height: usize,
) -> LogoRegion {
    mask_region_at(
        logo_mask(),
        local_x,
        local_y,
        viewport_width,
        viewport_height,
    )
}

fn logo_mask() -> &'static LogoMask {
    LOGO_MASK.get_or_init(|| generate_logo_mask(LOGO_MASK_SIZE))
}

fn mask_region_at(
    logo: &LogoMask,
    local_x: usize,
    local_y: usize,
    viewport_width: usize,
    viewport_height: usize,
) -> LogoRegion {
    if viewport_width == 0 || viewport_height == 0 {
        return LogoRegion::Empty;
    }

    let max_vx = usize_to_f32(viewport_width.saturating_sub(1).max(1));
    let max_vy = usize_to_f32(viewport_height.saturating_sub(1).max(1));
    let x = round_f32_to_usize(usize_to_f32(local_x) / max_vx * usize_to_f32(logo.width - 1));
    let y = round_f32_to_usize(usize_to_f32(local_y) / max_vy * usize_to_f32(logo.height - 1));
    logo.regions[y.min(logo.height - 1) * logo.width + x.min(logo.width - 1)]
}

#[cfg(test)]
fn region_bbox(logo: &LogoMask, region: LogoRegion) -> Option<(usize, usize, usize, usize)> {
    let mut min_x = usize::MAX;
    let mut min_y = usize::MAX;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut found = false;

    for y in 0..logo.height {
        for x in 0..logo.width {
            if logo.regions[y * logo.width + x] != region {
                continue;
            }
            found = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if found {
        Some((min_x, max_x, min_y, max_y))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_logo_has_symmetric_corner_triangles() {
        let mask = generate_logo_mask(256);
        let top = mask
            .regions
            .iter()
            .filter(|region| **region == LogoRegion::TopTriangle)
            .count();
        let bottom = mask
            .regions
            .iter()
            .filter(|region| **region == LogoRegion::BottomTriangle)
            .count();
        assert_eq!(top, bottom);

        let center_x = mask.width / 2;
        let center_y = mask.height / 2;
        assert_eq!(
            mask.regions[center_y * mask.width + center_x],
            LogoRegion::Empty
        );
        assert_eq!(
            mask.regions[(mask.height - 1) * mask.width + (mask.width - 1)],
            LogoRegion::BottomTriangle
        );
        assert_eq!(mask.regions[0], LogoRegion::TopTriangle);
    }

    #[test]
    fn top_and_bottom_triangle_boxes_have_matching_run_and_rise() {
        let mask = generate_logo_mask(512);
        let (top_min_x, top_max_x, top_min_y, top_max_y) =
            region_bbox(&mask, LogoRegion::TopTriangle).expect("top triangle missing");
        let (bottom_min_x, bottom_max_x, bottom_min_y, bottom_max_y) =
            region_bbox(&mask, LogoRegion::BottomTriangle).expect("bottom triangle missing");

        let top_w = top_max_x - top_min_x + 1;
        let top_h = top_max_y - top_min_y + 1;
        let bottom_w = bottom_max_x - bottom_min_x + 1;
        let bottom_h = bottom_max_y - bottom_min_y + 1;

        assert_eq!(top_w, top_h);
        assert_eq!(bottom_w, bottom_h);
        assert_eq!(top_w, bottom_w);
    }

    #[test]
    fn triangle_index_mapping_is_directional() {
        let n = 100usize;
        assert_eq!(triangle_nominal_index(0.0, n, LogoRegion::TopTriangle), 0);
        assert_eq!(
            triangle_nominal_index(1.0, n, LogoRegion::TopTriangle),
            n - 1
        );
        assert_eq!(
            triangle_nominal_index(0.0, n, LogoRegion::BottomTriangle),
            n - 1
        );
        assert_eq!(
            triangle_nominal_index(1.0, n, LogoRegion::BottomTriangle),
            0
        );
    }

    #[test]
    fn source_column_uses_shuffled_order() {
        let array = vec![2, 1, 3, 4, 5, 6, 7, 8];
        let n = array.len();
        let viewport_width = 80;

        let nominal_left = 0usize;
        let nominal_next = 1usize;
        let shuffled_left = shuffled_index_for_nominal(&array, nominal_left, n);
        let shuffled_next = shuffled_index_for_nominal(&array, nominal_next, n);

        let source_left =
            source_local_x_for_index(shuffled_left, n, LogoRegion::TopTriangle, viewport_width);
        let source_next =
            source_local_x_for_index(shuffled_next, n, LogoRegion::TopTriangle, viewport_width);

        assert!(source_left > source_next);
    }

    #[test]
    fn mask_region_lookup_tracks_generated_mask() {
        let logo = generate_logo_mask(128);
        let viewport_width = 64usize;
        let viewport_height = 64usize;

        let top = mask_region_at(&logo, 0, 0, viewport_width, viewport_height);
        let center = mask_region_at(
            &logo,
            viewport_width / 2,
            viewport_height / 2,
            viewport_width,
            viewport_height,
        );
        let bottom = mask_region_at(
            &logo,
            viewport_width - 1,
            viewport_height - 1,
            viewport_width,
            viewport_height,
        );

        assert_eq!(top, LogoRegion::TopTriangle);
        assert_eq!(center, LogoRegion::Empty);
        assert_eq!(bottom, LogoRegion::BottomTriangle);
    }
}
