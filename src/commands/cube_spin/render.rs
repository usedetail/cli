use ratatui::{style::Color, Frame};

use super::math3d::{Mat3, Vec3};

// ── logo texture ─────────────────────────────────────────────────────────────

/// Returns true when (u,v) in [0,1]² falls on a white stripe of the Detail logo.
/// Uses the same diagonal-band constants as logo_math.rs (d = u+v ∈ [0,2]).
fn is_logo_pixel(u: f32, v: f32) -> bool {
    let d = u + v;
    d <= 0.566
        || (0.734..=0.914).contains(&d)
        || (1.086..=1.266).contains(&d)
        || d >= 1.434
}

// ── camera (fixed) ───────────────────────────────────────────────────────────

pub struct Camera {
    // Pure frontal projection — no elevation, so the front face looks flat at θ=0.
    pub view_dir: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    // Separate light direction (elevated) used only for face shading.
    pub light_dir: Vec3,
}

impl Camera {
    pub fn new() -> Self {
        let view_dir = Vec3::new(0.0, 0.0, -1.0);
        let right = Vec3::new(1.0, 0.0, 0.0);
        let up = Vec3::new(0.0, 1.0, 0.0);
        // Light comes from 20° above the front — gives depth shading without tilting the view.
        let light_dir = Vec3::new(0.0, 0.342, 0.940);
        Self { view_dir, right, up, light_dir }
    }
}

// ── face definitions ─────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Face {
    axis: usize,  // 0=X, 1=Y, 2=Z
    sign: f32,    // +1 or -1 (which side of the plane)
}

const FACES: [Face; 6] = [
    Face { axis: 2, sign:  1.0 }, // +Z front
    Face { axis: 2, sign: -1.0 }, // -Z back
    Face { axis: 0, sign:  1.0 }, // +X right
    Face { axis: 0, sign: -1.0 }, // -X left
    Face { axis: 1, sign:  1.0 }, // +Y top
    Face { axis: 1, sign: -1.0 }, // -Y bottom
];

fn vec_component(v: Vec3, axis: usize) -> f32 {
    match axis { 0 => v.x, 1 => v.y, _ => v.z }
}

fn face_local_normal(face: &Face) -> Vec3 {
    let mut n = Vec3::ZERO;
    match face.axis {
        0 => n.x = face.sign,
        1 => n.y = face.sign,
        _ => n.z = face.sign,
    }
    n
}

/// Compute UV ∈ [0,1]² for a hit point on a face.
fn face_uv(face: &Face, hit: Vec3) -> (f32, f32) {
    match (face.axis, face.sign as i32) {
        (2,  1) => (hit.x + 0.5,  0.5 - hit.y),  // +Z front
        (2, -1) => (hit.x + 0.5,  0.5 - hit.y),  // -Z back (matches front so reset is seamless)
        (0,  1) => (-hit.z + 0.5, hit.y + 0.5),  // +X right
        (0, -1) => (hit.z + 0.5,  hit.y + 0.5),  // -X left
        (1,  1) => (hit.x + 0.5, -hit.z + 0.5),  // +Y top
        _       => (hit.x + 0.5,  hit.z + 0.5),  // -Y bottom
    }
}

// ── per-pixel ray cast ────────────────────────────────────────────────────────

struct Hit {
    shading: f32,
    logo_on: bool,
}

fn cast_ray(origin: Vec3, dir: Vec3, rotation: Mat3, cam: &Camera) -> Option<Hit> {
    let mut best_t = f32::MAX;
    let mut best_hit: Option<Hit> = None;

    for face in &FACES {
        let normal = face_local_normal(face);

        // Back-face cull — small epsilon prevents floating-point sin(π) ≈ -8.7e-8
        // from letting a nearly-edge-on side face sneak through as a thin sliver.
        let dot_nd = normal.dot(dir);
        if dot_nd >= -1e-4 {
            continue;
        }

        // Ray–plane intersection: find t where origin[axis] + t*dir[axis] = sign*0.5
        let denom = vec_component(dir, face.axis);
        if denom.abs() < 1e-8 {
            continue;
        }
        let t = (face.sign * 0.5 - vec_component(origin, face.axis)) / denom;
        if t <= 1e-4 || t >= best_t {
            continue;
        }

        // Check the hit is within the face bounds on the other two axes
        let hit = origin + dir * t;
        let in_bounds = match face.axis {
            0 => hit.y.abs() <= 0.5 && hit.z.abs() <= 0.5,
            1 => hit.x.abs() <= 0.5 && hit.z.abs() <= 0.5,
            _ => hit.x.abs() <= 0.5 && hit.y.abs() <= 0.5,
        };
        if !in_bounds {
            continue;
        }

        // Shading: dot of world-space face normal with camera position direction
        let world_normal = rotation.mul_vec(normal);
        let shading = world_normal.dot(cam.light_dir).max(0.0);

        let (u, v) = face_uv(face, hit);
        best_t = t;
        best_hit = Some(Hit { shading, logo_on: is_logo_pixel(u, v) });
    }

    best_hit
}

// ── viewport ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct CubeViewport {
    pub top: usize,
    pub left: usize,
    pub width: usize,
    pub height_rows: usize,
    pub side: usize,
}

pub fn compute_cube_viewport(cols: usize, rows: usize, aspect_x: f32) -> Option<CubeViewport> {
    const PAD_TOP: usize = 1;
    const PAD_SIDE: usize = 2;
    const ROW_UNITS: usize = 2;

    if cols == 0 || rows == 0 {
        return None;
    }
    let avail_h = rows.saturating_sub(PAD_TOP).saturating_mul(ROW_UNITS);
    let avail_w = cols.saturating_sub(PAD_SIDE * 2);
    if avail_h == 0 || avail_w == 0 {
        return None;
    }

    let side_from_w = (avail_w as f32 / aspect_x) as usize;
    let side = avail_h.min(side_from_w).max(1);
    let width = ((side as f32 * aspect_x).round() as usize).max(1).min(avail_w);

    Some(CubeViewport {
        top: PAD_TOP,
        left: PAD_SIDE + avail_w.saturating_sub(width) / 2,
        width,
        height_rows: side.div_ceil(ROW_UNITS),
        side,
    })
}

pub fn halfblocks_cell_aspect_x() -> f32 {
    detect_aspect().unwrap_or(1.0)
}

#[cfg(unix)]
fn detect_aspect() -> Option<f32> {
    let mut ws = libc::winsize { ws_row: 0, ws_col: 0, ws_xpixel: 0, ws_ypixel: 0 };
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if rc != 0 || ws.ws_col == 0 || ws.ws_row == 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 {
        return None;
    }
    let cw = f32::from(ws.ws_xpixel) / f32::from(ws.ws_col);
    let ch = f32::from(ws.ws_ypixel) / f32::from(ws.ws_row);
    if cw <= 0.0 || ch <= 0.0 { return None; }
    Some((ch / (2.0 * cw)).clamp(0.5, 4.0))
}

#[cfg(not(unix))]
fn detect_aspect() -> Option<f32> {
    None
}

// ── frame renderer ────────────────────────────────────────────────────────────

fn shade_to_color(shading: f32) -> Color {
    if shading > 0.7 { Color::White }
    else if shading > 0.3 { Color::Gray }
    else { Color::DarkGray }
}

pub fn render_cube_frame(f: &mut Frame<'_>, angle: f32, viewport: CubeViewport) {
    let rotation = Mat3::rotation(Vec3::new(1.0, -1.0, 0.0).normalize(), angle);
    let rot_inv = rotation.transpose(); // orthonormal so inverse = transpose
    let cam = Camera::new();

    let ray_dir = rot_inv.mul_vec(cam.view_dir);

    let buf = f.buffer_mut();

    let vp_right = viewport.left + viewport.width;
    let vp_bottom = viewport.top + viewport.height_rows;

    for row in viewport.top..vp_bottom {
        let pixel_y0 = (row - viewport.top) * 2;

        for col in viewport.left..vp_right {
            let local_x = col - viewport.left;

            let top_color = pixel_color(local_x, pixel_y0, viewport, &ray_dir, &rot_inv, &rotation, &cam);
            let bot_color = if pixel_y0 + 1 < viewport.side {
                pixel_color(local_x, pixel_y0 + 1, viewport, &ray_dir, &rot_inv, &rotation, &cam)
            } else {
                None
            };

            let (Ok(cx), Ok(cy)) = (u16::try_from(col), u16::try_from(row)) else {
                continue;
            };
            let Some(cell) = buf.cell_mut((cx, cy)) else {
                continue;
            };

            apply_halfblock(cell, top_color, bot_color);
        }
    }
}

fn pixel_color(
    local_x: usize,
    local_y: usize,
    viewport: CubeViewport,
    ray_dir: &Vec3,
    rot_inv: &Mat3,
    rotation: &Mat3,
    cam: &Camera,
) -> Option<Color> {
    // NDC: map to [-1, 1] with y flipped
    let nx = (local_x as f32 + 0.5) / viewport.width as f32 * 2.0 - 1.0;
    let ny = 1.0 - (local_y as f32 + 0.5) / viewport.side as f32 * 2.0;

    let origin_world = cam.right * nx + cam.up * ny;
    // Push the ray origin 2 units backward so it starts in front of the cube
    // (cube radius ≈ 0.866; without this the origin lands inside cube-local space
    // and face intersections all have negative t).
    let origin_cube = rot_inv.mul_vec(origin_world) - *ray_dir * 2.0;

    match cast_ray(origin_cube, *ray_dir, *rotation, cam) {
        None => None,
        Some(hit) if hit.logo_on => Some(shade_to_color(hit.shading)),
        Some(_) => Some(Color::Black),
    }
}

fn apply_halfblock(cell: &mut ratatui::buffer::Cell, top: Option<Color>, bot: Option<Color>) {
    match (top, bot) {
        (None, None) => {
            cell.set_char(' ').set_fg(Color::Reset).set_bg(Color::Reset);
        }
        (Some(t), None) => {
            cell.set_char('▀').set_fg(t).set_bg(Color::Reset);
        }
        (None, Some(b)) => {
            cell.set_char('▄').set_fg(b).set_bg(Color::Reset);
        }
        (Some(t), Some(b)) if t == b => {
            cell.set_char('█').set_fg(t).set_bg(Color::Reset);
        }
        (Some(t), Some(b)) => {
            cell.set_char('▀').set_fg(t).set_bg(b);
        }
    }
}
