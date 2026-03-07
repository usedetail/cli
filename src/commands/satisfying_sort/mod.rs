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
    Terminal,
};
use tokio::{signal, time::sleep};

mod logo_math;
mod numeric;
mod render;
mod sort_state;

use render::{compute_logo_viewport, halfblocks_cell_aspect_x, render_halfblocks_logo};
use sort_state::{sort_delay_ms, SortState};

const ARRAY_SIZE: usize = 1000;
const SPEED: u8 = 20;
const NOISE: u8 = 100;
const GREEN_SPEED: u8 = 50;
const LOOP_DELAY_MS: u64 = 2000;
const FRAME_MS: u64 = 16;
const LOGO_MASK_SIZE: usize = 512;

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    aspect_x: f32,
    last_size: (usize, usize),
}

fn restore_unowned_terminal_state() {
    let mut stdout = io::stdout();
    let _ = ratatui::crossterm::execute!(stdout, Show, LeaveAlternateScreen);
}

fn restore_terminal_state(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = ratatui::crossterm::execute!(terminal.backend_mut(), Show, LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        let mut stdout = io::stdout();
        if let Err(err) = ratatui::crossterm::execute!(stdout, EnterAlternateScreen, Hide) {
            restore_unowned_terminal_state();
            return Err(err.into());
        }
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(err) => {
                restore_unowned_terminal_state();
                return Err(err.into());
            }
        };
        if let Err(err) = terminal.clear() {
            restore_terminal_state(&mut terminal);
            return Err(err.into());
        }
        let initial_size = terminal
            .size()
            .map(|rect| (usize::from(rect.height), usize::from(rect.width)))
            .unwrap_or((0, 0));
        let aspect_x = if initial_size.0 > 0 && initial_size.1 > 0 {
            halfblocks_cell_aspect_x()
        } else {
            1.0
        };

        Ok(Self {
            terminal,
            aspect_x,
            last_size: initial_size,
        })
    }

    fn size(&self) -> (usize, usize) {
        self.terminal
            .size()
            .map(|rect| (usize::from(rect.height), usize::from(rect.width)))
            .unwrap_or((0, 0))
    }

    fn draw(&mut self, state: &SortState) -> Result<()> {
        let (rows, cols) = self.size();
        if rows == 0 || cols == 0 {
            return Ok(());
        }

        let size = (rows, cols);
        if self.last_size != size {
            self.aspect_x = halfblocks_cell_aspect_x();
            self.last_size = size;
        }

        let viewport = compute_logo_viewport(cols, rows, self.aspect_x);
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
        restore_terminal_state(&mut self.terminal);
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

    session.draw(&state)?;

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
        session.draw(&state)?;
    }

    Ok(())
}

async fn run_sort_pass(
    state: &mut SortState,
    session: &mut TerminalSession,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let n = state.len();
    let sort_delay_ms = sort_delay_ms();
    let sleep_every = (n / 200).max(1);
    let frame_every = (n / 100).max(1);

    for i in 0..n {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }

        state.apply_sort_step(i);
        session.draw(state)?;

        if sort_delay_ms > 0 && i % sleep_every == 0 {
            sleep_interruptible(Duration::from_millis(sort_delay_ms), stop).await;
        } else if i % frame_every == 0 {
            sleep_interruptible(Duration::from_millis(FRAME_MS), stop).await;
        }
    }

    state.finalize_sort_pass();

    Ok(())
}

async fn run_completion_pass(
    state: &mut SortState,
    session: &mut TerminalSession,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let n = state.len();
    let Some(last_index) = n.checked_sub(1) else {
        return Ok(());
    };
    let speed_divisor = 110_usize.saturating_sub(usize::from(GREEN_SPEED)).max(1);
    let bars_per_frame = (n / speed_divisor).max(1);

    let mut index = 0_usize;
    while index < n && !stop.load(Ordering::SeqCst) {
        let done = (index + bars_per_frame - 1).min(last_index);
        state.set_completion_index(done);
        session.draw(state)?;
        sleep_interruptible(Duration::from_millis(FRAME_MS), stop).await;
        index = index.saturating_add(bars_per_frame);
    }

    if !stop.load(Ordering::SeqCst) {
        state.set_completion_index(last_index);
        session.draw(state)?;
    }

    Ok(())
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
