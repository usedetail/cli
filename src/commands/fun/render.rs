use ratatui::{style::Color, Frame};

use super::logo_math::{
    logo_mask, mask_region_at, shuffled_index_for_nominal, source_local_x_for_index,
    triangle_nominal_index, LogoMask, LogoRegion,
};
use super::sort_state::{BandStyle, SortState};
const VIEWPORT_TOP_PADDING: usize = 1;
const VIEWPORT_SIDE_PADDING: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PixelStyle {
    Reset,
    Static,
    Triangle(BandStyle),
}

impl PixelStyle {
    fn from_band(style: BandStyle) -> Self {
        Self::Triangle(style)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LogoViewport {
    pub(super) top: usize,
    pub(super) left: usize,
    pub(super) width: usize,
    pub(super) height_rows: usize,
    pub(super) side: usize,
}

pub(super) fn compute_logo_viewport(
    width: usize,
    rows: usize,
    row_units: usize,
    aspect_x: f32,
) -> Option<LogoViewport> {
    if width == 0 || rows == 0 || row_units == 0 {
        return None;
    }

    let drawable_rows = rows;
    if drawable_rows == 0 {
        return None;
    }

    let viewport_top_padding = VIEWPORT_TOP_PADDING;
    let viewport_side_padding = VIEWPORT_SIDE_PADDING;
    let available_height = drawable_rows
        .saturating_sub(viewport_top_padding)
        .saturating_mul(row_units);
    let available_width = width.saturating_sub(viewport_side_padding.saturating_mul(2));
    if available_height == 0 || available_width == 0 {
        return None;
    }

    let side_from_width = ((available_width as f32) / aspect_x).floor() as usize;
    let side = available_height.min(side_from_width.max(1)).max(1);
    let viewport_width = ((side as f32) * aspect_x).round() as usize;
    let viewport_width = viewport_width.max(1).min(available_width);

    Some(LogoViewport {
        top: viewport_top_padding,
        left: viewport_side_padding + available_width.saturating_sub(viewport_width) / 2,
        width: viewport_width,
        height_rows: side.div_ceil(row_units),
        side,
    })
}

pub(super) fn halfblocks_cell_aspect_x() -> f32 {
    if let Some(detected) = detect_halfblocks_cell_aspect_x() {
        return detected;
    }

    1.0
}

#[cfg(unix)]
fn detect_halfblocks_cell_aspect_x() -> Option<f32> {
    let mut winsize = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut winsize) };
    if rc != 0
        || winsize.ws_col == 0
        || winsize.ws_row == 0
        || winsize.ws_xpixel == 0
        || winsize.ws_ypixel == 0
    {
        return None;
    }

    let char_w = winsize.ws_xpixel as f32 / winsize.ws_col as f32;
    let char_h = winsize.ws_ypixel as f32 / winsize.ws_row as f32;
    if char_w <= 0.0 || char_h <= 0.0 {
        return None;
    }

    Some((char_h / (2.0 * char_w)).clamp(0.5, 4.0))
}

#[cfg(not(unix))]
fn detect_halfblocks_cell_aspect_x() -> Option<f32> {
    None
}

pub(super) fn pixel_style_color(style: PixelStyle) -> Color {
    match style {
        PixelStyle::Reset => Color::Reset,
        PixelStyle::Static => Color::Reset,
        PixelStyle::Triangle(BandStyle::Idle) => Color::Reset,
        PixelStyle::Triangle(BandStyle::Active) => Color::Blue,
        PixelStyle::Triangle(BandStyle::Complete) => Color::Green,
    }
}

pub(super) fn render_halfblocks_logo(f: &mut Frame<'_>, state: &SortState, viewport: LogoViewport) {
    let buf = f.buffer_mut();

    rasterize_halfblocks(state, viewport, |x, y, top_style, bottom_style| {
        let top_on = top_style != PixelStyle::Reset;
        let bottom_on = bottom_style != PixelStyle::Reset;
        let top_color = pixel_style_color(top_style);
        let bottom_color = pixel_style_color(bottom_style);
        let Some(cell) = buf.cell_mut((x as u16, y as u16)) else {
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
    logo: &'a LogoMask,
    n: usize,
    viewport_width: usize,
    viewport_height: usize,
    x_den: f32,
}

pub(super) fn rasterize_halfblocks(
    state: &SortState,
    viewport: LogoViewport,
    mut visit: impl FnMut(usize, usize, PixelStyle, PixelStyle),
) {
    if viewport.width == 0 || viewport.side == 0 || viewport.height_rows == 0 {
        return;
    }

    let logo = logo_mask();
    let n = state.array.len().max(1);
    let viewport_right = viewport.left.saturating_add(viewport.width);
    let viewport_bottom = viewport.top.saturating_add(viewport.height_rows);
    let x_den = viewport.width.saturating_sub(1).max(1) as f32;
    let pixel_ctx = PixelQueryCtx {
        state,
        logo,
        n,
        viewport_width: viewport.width,
        viewport_height: viewport.side,
        x_den,
    };

    for y in viewport.top..viewport_bottom {
        let local_y = (y - viewport.top).saturating_mul(2);
        for x in viewport.left..viewport_right {
            let local_x = x - viewport.left;
            let top_style = pixel_style_at(&pixel_ctx, local_x, local_y);
            let bottom_style = if local_y + 1 < viewport.side {
                pixel_style_at(&pixel_ctx, local_x, local_y + 1)
            } else {
                PixelStyle::Reset
            };
            visit(x, y, top_style, bottom_style);
        }
    }
}

fn pixel_style_at(ctx: &PixelQueryCtx<'_>, local_x: usize, local_y: usize) -> PixelStyle {
    let region = mask_region_at(
        ctx.logo,
        local_x,
        local_y,
        ctx.viewport_width,
        ctx.viewport_height,
    );
    if region == LogoRegion::Empty {
        return PixelStyle::Reset;
    }

    if matches!(region, LogoRegion::TopTriangle | LogoRegion::BottomTriangle) {
        let nx = local_x as f32 / ctx.x_den;
        let nominal_index = triangle_nominal_index(nx, ctx.n, region);
        let shuffled_index = visual_source_index_for_nominal(ctx.state, nominal_index, ctx.n);
        let source_x = source_local_x_for_index(shuffled_index, ctx.n, region, ctx.viewport_width);
        let source_region = mask_region_at(
            ctx.logo,
            source_x,
            local_y,
            ctx.viewport_width,
            ctx.viewport_height,
        );
        if source_region != region {
            return PixelStyle::Reset;
        }
        return PixelStyle::from_band(ctx.state.style_for_index(nominal_index));
    }

    PixelStyle::Static
}

pub(super) fn visual_source_index_for_nominal(
    state: &SortState,
    nominal_index: usize,
    n: usize,
) -> usize {
    let n = n.max(1);
    let nominal_index = nominal_index.min(n - 1);

    if state.scan_complete
        || state
            .current_scan_index
            .is_some_and(|scan| nominal_index <= scan)
    {
        return nominal_index;
    }

    shuffled_index_for_nominal(&state.source_array, nominal_index, n)
}

#[cfg(test)]
pub(super) fn half_block_cell(top: PixelStyle, bottom: PixelStyle) -> (PixelStyle, char) {
    if top == PixelStyle::Reset && bottom == PixelStyle::Reset {
        return (PixelStyle::Reset, ' ');
    }

    match (top, bottom) {
        (PixelStyle::Reset, style) => (style, '▄'),
        (style, PixelStyle::Reset) => (style, '▀'),
        (a, b) if a == b => (a, '█'),
        (top_style, _) => (top_style, '▀'),
    }
}
