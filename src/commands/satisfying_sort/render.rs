use ratatui::{style::Color, Frame};

use super::logo_math::{
    logo_region_at, shuffled_index_for_nominal, source_local_x_for_index, triangle_nominal_index,
    LogoRegion,
};
use super::numeric::{floor_f32_to_usize, round_f32_to_usize, usize_to_f32};
use super::sort_state::{BandStyle, SortState};

const VIEWPORT_TOP_PADDING: usize = 1;
const VIEWPORT_SIDE_PADDING: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PixelStyle {
    Off,
    Base,
    Active,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LogoViewport {
    top: usize,
    left: usize,
    width: usize,
    height_rows: usize,
    side: usize,
}

pub(super) fn compute_logo_viewport(
    width: usize,
    rows: usize,
    aspect_x: f32,
) -> Option<LogoViewport> {
    const ROW_UNITS: usize = 2;
    if width == 0 || rows == 0 {
        return None;
    }

    let viewport_top_padding = VIEWPORT_TOP_PADDING;
    let viewport_side_padding = VIEWPORT_SIDE_PADDING;
    let available_height = rows
        .saturating_sub(viewport_top_padding)
        .saturating_mul(ROW_UNITS);
    let available_width = width.saturating_sub(viewport_side_padding.saturating_mul(2));
    if available_height == 0 || available_width == 0 {
        return None;
    }

    let side_from_width = floor_f32_to_usize(usize_to_f32(available_width) / aspect_x);
    let side = available_height.min(side_from_width).max(1);
    let viewport_width = round_f32_to_usize(usize_to_f32(side) * aspect_x);
    let viewport_width = viewport_width.max(1).min(available_width);

    Some(LogoViewport {
        top: viewport_top_padding,
        left: viewport_side_padding + available_width.saturating_sub(viewport_width) / 2,
        width: viewport_width,
        height_rows: side.div_ceil(ROW_UNITS),
        side,
    })
}

pub(super) fn halfblocks_cell_aspect_x() -> f32 {
    detect_halfblocks_cell_aspect_x().unwrap_or(1.0)
}

#[cfg(unix)]
fn detect_halfblocks_cell_aspect_x() -> Option<f32> {
    let mut winsize = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    #[allow(
        unsafe_code,
        reason = "libc ioctl required for terminal pixel dimensions"
    )]
    // SAFETY: `winsize` is a valid, zeroed struct and `TIOCGWINSZ` is the standard ioctl for
    // querying terminal dimensions. The kernel writes into the provided pointer.
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize) };
    if rc != 0
        || winsize.ws_col == 0
        || winsize.ws_row == 0
        || winsize.ws_xpixel == 0
        || winsize.ws_ypixel == 0
    {
        return None;
    }

    let char_w = f32::from(winsize.ws_xpixel) / f32::from(winsize.ws_col);
    let char_h = f32::from(winsize.ws_ypixel) / f32::from(winsize.ws_row);
    if char_w <= 0.0 || char_h <= 0.0 {
        return None;
    }

    Some((char_h / (2.0 * char_w)).clamp(0.5, 4.0))
}

#[cfg(not(unix))]
fn detect_halfblocks_cell_aspect_x() -> Option<f32> {
    None
}

const fn pixel_style_color(style: PixelStyle) -> Color {
    match style {
        PixelStyle::Off | PixelStyle::Base => Color::Reset,
        PixelStyle::Active => Color::Blue,
        PixelStyle::Complete => Color::Green,
    }
}

pub(super) fn render_halfblocks_logo(f: &mut Frame<'_>, state: &SortState, viewport: LogoViewport) {
    let buf = f.buffer_mut();

    rasterize_halfblocks(state, viewport, |x, y, top_style, bottom_style| {
        let top_on = top_style != PixelStyle::Off;
        let bottom_on = bottom_style != PixelStyle::Off;
        let top_color = pixel_style_color(top_style);
        let bottom_color = pixel_style_color(bottom_style);
        let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
            return;
        };
        let Some(cell) = buf.cell_mut((x, y)) else {
            return;
        };

        match (top_on, bottom_on) {
            (false, false) => {
                cell.set_char(' ').set_fg(Color::Reset).set_bg(Color::Reset);
            }
            (true, true) => {
                if top_style == bottom_style {
                    cell.set_char('█').set_fg(top_color).set_bg(Color::Reset);
                } else {
                    cell.set_char('▀').set_fg(top_color).set_bg(bottom_color);
                }
            }
            (true, false) => {
                cell.set_char('▀').set_fg(top_color).set_bg(Color::Reset);
            }
            (false, true) => {
                cell.set_char('▄').set_fg(bottom_color).set_bg(Color::Reset);
            }
        }
    });
}

struct PixelQueryCtx<'a> {
    state: &'a SortState,
    n: usize,
    viewport_width: usize,
    viewport_height: usize,
    x_den: f32,
    active_min_window: usize,
}

fn rasterize_halfblocks(
    state: &SortState,
    viewport: LogoViewport,
    mut visit: impl FnMut(usize, usize, PixelStyle, PixelStyle),
) {
    if viewport.width == 0 || viewport.side == 0 || viewport.height_rows == 0 {
        return;
    }

    let n = state.len().max(1);
    let viewport_right = viewport.left.saturating_add(viewport.width);
    let viewport_bottom = viewport.top.saturating_add(viewport.height_rows);
    let x_den = usize_to_f32(viewport.width.saturating_sub(1).max(1));
    let active_min_window = min_active_window_for_columns(n, viewport.width, 2);
    let pixel_ctx = PixelQueryCtx {
        state,
        n,
        viewport_width: viewport.width,
        viewport_height: viewport.side,
        x_den,
        active_min_window,
    };

    for y in viewport.top..viewport_bottom {
        let local_y = (y - viewport.top).saturating_mul(2);
        for x in viewport.left..viewport_right {
            let local_x = x - viewport.left;
            let top_style = pixel_style_at(&pixel_ctx, local_x, local_y);
            let bottom_style = if local_y + 1 < viewport.side {
                pixel_style_at(&pixel_ctx, local_x, local_y + 1)
            } else {
                PixelStyle::Off
            };
            visit(x, y, top_style, bottom_style);
        }
    }
}

fn pixel_style_at(ctx: &PixelQueryCtx<'_>, local_x: usize, local_y: usize) -> PixelStyle {
    let region = logo_region_at(local_x, local_y, ctx.viewport_width, ctx.viewport_height);
    if region == LogoRegion::Empty {
        return PixelStyle::Off;
    }

    if matches!(region, LogoRegion::TopTriangle | LogoRegion::BottomTriangle) {
        let nx = usize_to_f32(local_x) / ctx.x_den;
        let nominal_index = triangle_nominal_index(nx, ctx.n, region);
        let shuffled_index = visual_source_index_for_nominal(ctx.state, nominal_index, ctx.n);
        let source_x = source_local_x_for_index(shuffled_index, ctx.n, region, ctx.viewport_width);
        let source_region =
            logo_region_at(source_x, local_y, ctx.viewport_width, ctx.viewport_height);
        if source_region != region {
            return PixelStyle::Off;
        }
        return match ctx
            .state
            .style_for_index_with_min_window(nominal_index, ctx.active_min_window)
        {
            BandStyle::Idle => PixelStyle::Base,
            BandStyle::Active => PixelStyle::Active,
            BandStyle::Complete => PixelStyle::Complete,
        };
    }

    PixelStyle::Base
}

fn visual_source_index_for_nominal(state: &SortState, nominal_index: usize, n: usize) -> usize {
    let n = n.max(1);
    let nominal_index = nominal_index.min(n - 1);

    if state.scan_complete()
        || state
            .current_scan_index()
            .is_some_and(|scan| nominal_index <= scan)
    {
        return nominal_index;
    }

    shuffled_index_for_nominal(state.source_array(), nominal_index, n)
}

fn min_active_window_for_columns(n: usize, viewport_width: usize, target_columns: usize) -> usize {
    let n = n.max(1);
    let target_columns = target_columns.max(1);
    if viewport_width <= 1 {
        return n.min(target_columns);
    }

    let x_den = usize_to_f32(viewport_width.saturating_sub(1).max(1));
    let mut prev = triangle_nominal_index(0.0, n, LogoRegion::TopTriangle);
    let mut max_step = 1_usize;

    for x in 1..viewport_width {
        let nx = usize_to_f32(x) / x_den;
        let idx = triangle_nominal_index(nx, n, LogoRegion::TopTriangle);
        max_step = max_step.max(idx.saturating_sub(prev));
        prev = idx;
    }

    max_step.saturating_mul(target_columns).clamp(1, n)
}

#[cfg(test)]
fn half_block_cell(top: PixelStyle, bottom: PixelStyle) -> (PixelStyle, char) {
    if top == PixelStyle::Off && bottom == PixelStyle::Off {
        return (PixelStyle::Off, ' ');
    }

    match (top, bottom) {
        (PixelStyle::Off, style) => (style, '▄'),
        (style, PixelStyle::Off) => (style, '▀'),
        (a, b) if a == b => (a, '█'),
        (top_style, _) => (top_style, '▀'),
    }
}

#[cfg(test)]
mod tests {
    use rand::{rngs::SmallRng, SeedableRng};

    use super::*;

    #[test]
    fn half_block_cells_use_block_glyphs_without_color() {
        let (_, up) = half_block_cell(PixelStyle::Base, PixelStyle::Off);
        let (_, down) = half_block_cell(PixelStyle::Off, PixelStyle::Base);
        let (_, full) = half_block_cell(PixelStyle::Base, PixelStyle::Base);
        assert_eq!(up, '▀');
        assert_eq!(down, '▄');
        assert_eq!(full, '█');
    }

    #[test]
    fn compute_logo_viewport_is_square_in_unit_space() {
        let viewport = compute_logo_viewport(140, 50, 1.0).expect("viewport");
        assert!(viewport.width > 0);
        assert!(viewport.height_rows > 0);
        assert_eq!(viewport.height_rows, viewport.side.div_ceil(2));
    }

    #[test]
    fn compute_logo_viewport_none_when_rows_too_small_for_padding() {
        let viewport = compute_logo_viewport(80, 1, 1.0);
        assert!(viewport.is_none());
    }

    #[test]
    fn compute_logo_viewport_respects_fixed_padding() {
        let viewport = compute_logo_viewport(80, 24, 1.0).expect("viewport");
        assert_eq!(viewport.top, 1);
        assert!(viewport.left >= 2);
    }

    #[test]
    fn rasterize_halfblocks_visits_exact_viewport_area() {
        let mut rng = SmallRng::seed_from_u64(11);
        let state = SortState::new(64, &mut rng);
        let viewport = LogoViewport {
            top: 3,
            left: 5,
            width: 17,
            height_rows: 9,
            side: 18,
        };
        let mut count = 0_usize;

        rasterize_halfblocks(&state, viewport, |x, y, _, _| {
            assert!((5..22).contains(&x));
            assert!((3..12).contains(&y));
            count = count.saturating_add(1);
        });

        assert_eq!(count, viewport.width * viewport.height_rows);
    }

    #[test]
    fn halfblock_palette_matches_requested_defaults() {
        assert_eq!(pixel_style_color(PixelStyle::Base), Color::Reset);
        assert_eq!(pixel_style_color(PixelStyle::Active), Color::Blue);
        assert_eq!(pixel_style_color(PixelStyle::Complete), Color::Green);
    }

    #[test]
    fn halfblocks_aspect_is_in_valid_range() {
        let aspect = halfblocks_cell_aspect_x();
        assert!((0.5..=4.0).contains(&aspect));
    }

    #[test]
    fn min_active_window_produces_two_visible_blue_columns() {
        let n = 1000_usize;
        for width in [80_usize, 120, 160, 220] {
            let x_den = usize_to_f32(width.saturating_sub(1).max(1));
            let nominal_by_col: Vec<usize> = (0..width)
                .map(|x| {
                    triangle_nominal_index(usize_to_f32(x) / x_den, n, LogoRegion::TopTriangle)
                })
                .collect();

            let scan_start = nominal_by_col[width / 4].min(n - 1);
            let min_window = min_active_window_for_columns(n, width, 2);

            let mut rng = SmallRng::seed_from_u64(17);
            let mut state = SortState::new(n, &mut rng);
            state.apply_sort_step(scan_start);

            let active_cols = nominal_by_col
                .iter()
                .filter(|&&idx| {
                    state.style_for_index_with_min_window(idx, min_window) == BandStyle::Active
                })
                .count();

            assert!(
                active_cols >= 2,
                "width={width} min_window={min_window} active_cols={active_cols}"
            );
        }
    }
}
