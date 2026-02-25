use std::f32::consts::PI;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::cursor::{Hide, Show},
    crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    widgets::Clear,
    Terminal,
};
use tokio::{signal, time::sleep};

mod math3d;
mod render;

use render::{compute_cube_viewport, halfblocks_cell_aspect_x, render_cube_frame};

const TOTAL_FRAMES: usize = 120;
const FRAME_MS: u64 = 16;
const PAUSE_MS: u64 = 600;

struct Session {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    aspect_x: f32,
}

impl Session {
    fn enter() -> Result<Self> {
        let mut stdout = io::stdout();
        ratatui::crossterm::execute!(stdout, EnterAlternateScreen, Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        let aspect_x = halfblocks_cell_aspect_x();
        Ok(Self { terminal, aspect_x })
    }

    fn size(&self) -> (usize, usize) {
        self.terminal
            .size()
            .map(|r| (r.height as usize, r.width as usize))
            .unwrap_or((0, 0))
    }

    fn draw(&mut self, angle: f32) -> Result<()> {
        let (rows, cols) = self.size();
        if rows == 0 || cols == 0 {
            return Ok(());
        }
        let aspect_x = self.aspect_x;
        let Some(vp) = compute_cube_viewport(cols, rows, aspect_x) else {
            return Ok(());
        };
        self.terminal.draw(|f| {
            f.render_widget(Clear, f.area());
            render_cube_frame(f, angle, vp);
        })?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = ratatui::crossterm::execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

pub async fn handle() -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        stop2.store(true, Ordering::SeqCst);
    });

    let mut session = Session::enter()?;

    // Always animate the same 0→π arc. Front and back faces look identical,
    // so resetting to 0 after each pause is seamless.
    session.draw(0.0)?;
    pause_interruptible(PAUSE_MS, &stop).await;
    if stop.load(Ordering::SeqCst) {
        return Ok(());
    }

    loop {
        for frame in 0..TOTAL_FRAMES {
            if stop.load(Ordering::SeqCst) {
                return Ok(());
            }
            let t = frame as f32 / (TOTAL_FRAMES - 1) as f32;
            session.draw(PI * t)?;
            sleep(Duration::from_millis(FRAME_MS)).await;
        }

        pause_interruptible(PAUSE_MS, &stop).await;
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
    }
}

async fn pause_interruptible(ms: u64, stop: &Arc<AtomicBool>) {
    let chunk = Duration::from_millis(25);
    let mut remaining = Duration::from_millis(ms);
    while remaining > Duration::ZERO && !stop.load(Ordering::SeqCst) {
        let step = remaining.min(chunk);
        sleep(step).await;
        remaining = remaining.saturating_sub(step);
    }
}
