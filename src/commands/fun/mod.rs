use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rand::{rngs::SmallRng, SeedableRng};
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        cursor::{Hide, Show},
        terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    },
    widgets::Clear,
    Terminal as RatatuiTerminal,
};
use tokio::{signal, time::sleep};

mod logo_math;
mod render;
mod sort_state;

use render::{compute_logo_viewport, halfblocks_cell_aspect_x, render_halfblocks_logo};
use sort_state::{place_target_value, sort_delay_ms, SortState};

#[cfg(test)]
use logo_math::{
    mask_region_at, region_bbox, shuffled_index_for_nominal, source_local_x_for_index,
    triangle_nominal_index, LogoMask, LogoRegion, TOP_TRIANGLE_X_SPAN,
};
#[cfg(test)]
use ratatui::style::Color;
#[cfg(test)]
use render::{
    half_block_cell, pixel_style_color, rasterize_halfblocks, visual_source_index_for_nominal,
    LogoViewport, PixelStyle,
};
#[cfg(test)]
use sort_state::{generate_array, BandStyle};

const ARRAY_SIZE: usize = 1000;
const SPEED: u8 = 20;
const NOISE: u8 = 100;
const GREEN_SPEED: u8 = 50;
const LOOP_DELAY_MS: u64 = 2000;
const FRAME_MS: u64 = 16;
const LOGO_MASK_SIZE: usize = 512;

struct TerminalSession {
    terminal: RatatuiTerminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        let mut stdout = io::stdout();
        ratatui::crossterm::execute!(stdout, EnterAlternateScreen, Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = RatatuiTerminal::new(backend)?;
        terminal.clear()?;

        Ok(Self { terminal })
    }

    fn size(&self) -> (usize, usize) {
        self.terminal
            .size()
            .map(|rect| (usize::from(rect.height), usize::from(rect.width)))
            .unwrap_or((0, 0))
    }

    fn draw_halfblocks(&mut self, state: &SortState, rows: usize, cols: usize) -> Result<()> {
        let viewport = compute_logo_viewport(cols, rows, 2, halfblocks_cell_aspect_x());

        self.terminal.draw(|f| {
            f.render_widget(Clear, f.area());
            if let Some(viewport) = viewport {
                render_halfblocks_logo(f, state, viewport);
            }
        })?;

        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ =
            ratatui::crossterm::execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

impl TerminalSession {
    fn draw(&mut self, state: &SortState) -> Result<()> {
        let (rows, cols) = self.size();
        if rows == 0 || cols == 0 {
            return Ok(());
        }

        self.draw_halfblocks(state, rows, cols)
    }
}

pub async fn handle() -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_signal = Arc::clone(&stop);
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        stop_for_signal.store(true, Ordering::SeqCst);
    });

    let mut session = TerminalSession::enter()?;
    let mut rng = SmallRng::seed_from_u64(rand::random());
    let mut state = SortState::new(ARRAY_SIZE, &mut rng);

    render_current_frame(&mut session, &state)?;

    while !stop.load(Ordering::SeqCst) {
        run_sort_pass(&mut state, &mut session, &stop).await?;
        if stop.load(Ordering::SeqCst) {
            break;
        }

        run_completion_pass(&mut state, &mut session, &stop).await?;
        if stop.load(Ordering::SeqCst) {
            break;
        }

        sleep_interruptible(Duration::from_millis(LOOP_DELAY_MS), &stop).await;
        state.reset(&mut rng);
        render_current_frame(&mut session, &state)?;
    }

    Ok(())
}

async fn run_sort_pass(
    state: &mut SortState,
    session: &mut TerminalSession,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let n = state.array.len();
    let sort_delay_ms = sort_delay_ms();
    let sleep_every = (n / 200).max(1);
    let frame_every = (n / 100).max(1);

    for i in 0..n {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }

        state.current_window_size = state.base_window_size.max(1);
        state.current_scan_index = Some(i);
        place_target_value(&mut state.array, i);
        render_current_frame(session, state)?;

        if sort_delay_ms > 0 && i % sleep_every == 0 {
            sleep_interruptible(Duration::from_millis(sort_delay_ms), stop).await;
        } else if i % frame_every == 0 {
            sleep_interruptible(Duration::from_millis(FRAME_MS), stop).await;
        }
    }

    state.scan_complete = true;
    state.current_scan_index = None;
    state.current_window_size = 0;
    state.complete_scan_index = None;

    Ok(())
}

async fn run_completion_pass(
    state: &mut SortState,
    session: &mut TerminalSession,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let n = state.array.len();
    let speed_divisor = 110usize.saturating_sub(usize::from(GREEN_SPEED)).max(1);
    let bars_per_frame = (n / speed_divisor).max(1);

    let mut index = 0usize;
    while index < n && !stop.load(Ordering::SeqCst) {
        let done = (index + bars_per_frame - 1).min(n - 1);
        state.complete_scan_index = Some(done);
        render_current_frame(session, state)?;
        sleep_interruptible(Duration::from_millis(FRAME_MS), stop).await;
        index = index.saturating_add(bars_per_frame);
    }

    if !stop.load(Ordering::SeqCst) {
        state.complete_scan_index = Some(n - 1);
        render_current_frame(session, state)?;
    }

    Ok(())
}

fn render_current_frame(session: &mut TerminalSession, state: &SortState) -> Result<()> {
    session.draw(state)
}

async fn sleep_interruptible(duration: Duration, stop: &Arc<AtomicBool>) {
    let chunk = Duration::from_millis(25);
    let mut remaining = duration;

    while remaining > Duration::ZERO && !stop.load(Ordering::SeqCst) {
        let current = if remaining > chunk { chunk } else { remaining };
        sleep(current).await;
        remaining = remaining.checked_sub(current).unwrap_or(Duration::ZERO);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_array_is_permutation() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut array = generate_array(128, &mut rng);
        array.sort_unstable();
        let expected: Vec<usize> = (1..=128).collect();
        assert_eq!(array, expected);
    }

    #[test]
    fn place_target_value_moves_target_into_place() {
        let mut array = vec![3, 2, 1, 4];
        place_target_value(&mut array, 0);
        assert_eq!(array, vec![1, 2, 3, 4]);
    }

    #[test]
    fn style_for_index_respects_active_window() {
        let mut rng = SmallRng::seed_from_u64(1);
        let mut state = SortState::new(64, &mut rng);
        state.scan_complete = false;
        state.current_scan_index = Some(10);
        state.current_window_size = 8;

        assert_eq!(state.style_for_index(11), BandStyle::Active);
        assert_eq!(state.style_for_index(12), BandStyle::Active);
        assert_eq!(state.style_for_index(14), BandStyle::Active);
        assert_eq!(state.style_for_index(15), BandStyle::Active);
    }

    #[test]
    fn style_for_index_marks_completion() {
        let mut rng = SmallRng::seed_from_u64(9);
        let mut state = SortState::new(32, &mut rng);
        state.scan_complete = true;
        state.complete_scan_index = Some(5);

        assert_eq!(state.style_for_index(3), BandStyle::Complete);
        assert_eq!(state.style_for_index(7), BandStyle::Idle);
    }

    #[test]
    fn generated_logo_has_symmetric_corner_triangles() {
        let mask = LogoMask::generated(256);
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
        let mask = LogoMask::generated(512);
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
    fn triangle_style_uses_nominal_column_order() {
        let mut rng = SmallRng::seed_from_u64(13);
        let mut state = SortState::new(8, &mut rng);
        state.array = vec![2, 1, 3, 4, 5, 6, 7, 8];
        state.scan_complete = false;
        state.current_scan_index = Some(0);
        state.current_window_size = 1;

        let nx_for_nominal_zero = TOP_TRIANGLE_X_SPAN * (0.5 / state.array.len() as f32);
        let nx_for_nominal_one = TOP_TRIANGLE_X_SPAN * (1.5 / state.array.len() as f32);

        assert_eq!(
            state.style_for_index(triangle_nominal_index(
                nx_for_nominal_zero,
                state.array.len(),
                LogoRegion::TopTriangle
            )),
            BandStyle::Active
        );
        assert_eq!(
            state.style_for_index(triangle_nominal_index(
                nx_for_nominal_one,
                state.array.len(),
                LogoRegion::TopTriangle
            )),
            BandStyle::Idle
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
        let logo = LogoMask::generated(128);
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

    #[test]
    fn half_block_cells_use_block_glyphs_without_color() {
        let (_, up) = half_block_cell(PixelStyle::Static, PixelStyle::Reset);
        let (_, down) = half_block_cell(PixelStyle::Reset, PixelStyle::Static);
        let (_, full) = half_block_cell(PixelStyle::Static, PixelStyle::Static);
        assert_eq!(up, '▀');
        assert_eq!(down, '▄');
        assert_eq!(full, '█');
    }

    #[test]
    fn compute_logo_viewport_is_square_in_unit_space() {
        let viewport = compute_logo_viewport(140, 50, 2, 1.0).expect("viewport");
        assert!(viewport.width > 0);
        assert!(viewport.height_rows > 0);
        assert_eq!(viewport.height_rows, viewport.side.div_ceil(2));
    }

    #[test]
    fn compute_logo_viewport_none_when_rows_too_small_for_padding() {
        let viewport = compute_logo_viewport(80, 1, 2, 1.0);
        assert!(viewport.is_none());
    }

    #[test]
    fn visual_mapping_does_not_shift_unreached_columns() {
        let mut rng = SmallRng::seed_from_u64(23);
        let mut state = SortState::new(8, &mut rng);
        state.source_array = vec![2, 1, 4, 3, 5, 6, 8, 7];
        state.array = state.source_array.clone();
        state.scan_complete = false;
        state.current_scan_index = Some(2);

        assert_eq!(visual_source_index_for_nominal(&state, 1, 8), 1);
        assert_eq!(
            visual_source_index_for_nominal(&state, 3, 8),
            shuffled_index_for_nominal(&state.source_array, 3, 8)
        );

        state.array.swap(2, 6);
        assert_eq!(
            visual_source_index_for_nominal(&state, 6, 8),
            shuffled_index_for_nominal(&state.source_array, 6, 8)
        );

        state.scan_complete = true;
        assert_eq!(visual_source_index_for_nominal(&state, 6, 8), 6);
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
        let mut count = 0usize;

        rasterize_halfblocks(&state, viewport, |x, y, _, _| {
            assert!((5..22).contains(&x));
            assert!((3..12).contains(&y));
            count = count.saturating_add(1);
        });

        assert_eq!(count, viewport.width * viewport.height_rows);
    }

    #[test]
    fn halfblock_palette_matches_requested_defaults() {
        assert_eq!(pixel_style_color(PixelStyle::Static), Color::Reset);
        assert_eq!(
            pixel_style_color(PixelStyle::Triangle(BandStyle::Idle)),
            Color::Reset
        );
        assert_eq!(
            pixel_style_color(PixelStyle::Triangle(BandStyle::Active)),
            Color::Blue
        );
        assert_eq!(
            pixel_style_color(PixelStyle::Triangle(BandStyle::Complete)),
            Color::Green
        );
    }

    #[test]
    fn compute_logo_viewport_respects_fixed_padding() {
        let viewport = compute_logo_viewport(80, 24, 2, 1.0).expect("viewport");
        assert_eq!(viewport.top, 1);
        assert!(viewport.left >= 2);
    }
}
