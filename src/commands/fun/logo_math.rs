use std::sync::OnceLock;

use super::LOGO_MASK_SIZE;

const TOP_TRIANGLE_DIAG_MAX: f32 = 0.566;
const CENTER_BAND_A_MIN: f32 = 0.734;
const CENTER_BAND_A_MAX: f32 = 0.914;
const CENTER_BAND_B_MIN: f32 = 1.086;
const CENTER_BAND_B_MAX: f32 = 1.266;
const BOTTOM_TRIANGLE_DIAG_MIN: f32 = 1.434;
pub(super) const TOP_TRIANGLE_X_SPAN: f32 = TOP_TRIANGLE_DIAG_MAX;
pub(super) const BOTTOM_TRIANGLE_X_SPAN: f32 = TOP_TRIANGLE_DIAG_MAX;

static LOGO_MASK: OnceLock<LogoMask> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LogoRegion {
    Empty,
    Static,
    TopTriangle,
    BottomTriangle,
}

#[derive(Clone)]
pub(super) struct LogoMask {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) regions: Vec<LogoRegion>,
}

impl LogoMask {
    fn from_embedded_or_fallback() -> Self {
        Self::generated(LOGO_MASK_SIZE)
    }

    pub(super) fn generated(size: usize) -> Self {
        let side = size.max(64);
        let mut regions = vec![LogoRegion::Empty; side * side];
        let denom = (side.saturating_sub(1).max(1)) as f32;

        for y in 0..side {
            for x in 0..side {
                let d = (x as f32 + y as f32) / denom;
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

        Self {
            width: side,
            height: side,
            regions,
        }
    }
}

pub(super) fn logo_mask() -> &'static LogoMask {
    LOGO_MASK.get_or_init(LogoMask::from_embedded_or_fallback)
}

pub(super) fn triangle_nominal_index(nx: f32, n: usize, region: LogoRegion) -> usize {
    let n = n.max(1);
    let span = match region {
        LogoRegion::TopTriangle => TOP_TRIANGLE_X_SPAN,
        LogoRegion::BottomTriangle => BOTTOM_TRIANGLE_X_SPAN,
        _ => 1.0,
    };
    let direction_x = match region {
        LogoRegion::TopTriangle => nx,
        LogoRegion::BottomTriangle => 1.0 - nx,
        _ => 0.0,
    };
    let t = (direction_x / span).clamp(0.0, 1.0);
    ((t * n as f32).floor() as usize).min(n - 1)
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
        shuffled_index.min(n - 1) as f32 / (n - 1) as f32
    };

    let nx = match region {
        LogoRegion::TopTriangle => (unit * TOP_TRIANGLE_X_SPAN).clamp(0.0, 1.0),
        LogoRegion::BottomTriangle => (1.0 - unit * BOTTOM_TRIANGLE_X_SPAN).clamp(0.0, 1.0),
        LogoRegion::Static | LogoRegion::Empty => unit.clamp(0.0, 1.0),
    };
    ((nx * max_col as f32).round() as usize).min(max_col)
}

pub(super) fn mask_region_at(
    logo: &LogoMask,
    local_x: usize,
    local_y: usize,
    viewport_width: usize,
    viewport_height: usize,
) -> LogoRegion {
    if viewport_width == 0 || viewport_height == 0 {
        return LogoRegion::Empty;
    }

    let x = (((local_x as f32 + 0.5) * logo.width as f32) / viewport_width as f32).floor() as usize;
    let y =
        (((local_y as f32 + 0.5) * logo.height as f32) / viewport_height as f32).floor() as usize;
    logo.regions[y.min(logo.height - 1) * logo.width + x.min(logo.width - 1)]
}

#[cfg(test)]
pub(super) fn region_bbox(
    logo: &LogoMask,
    region: LogoRegion,
) -> Option<(usize, usize, usize, usize)> {
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
