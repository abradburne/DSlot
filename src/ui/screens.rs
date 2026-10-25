//! SDL, the windows and their renderers: everything the shelf needs before it needs the games,
//! so it can be made while they are listed. They are let go of while a game runs (the game
//! needs the panels) and made again when it exits.

use super::layout::{View, SH, SW};
use super::Opts;
use nibkit::screens::{self, Placement};
use sdl2::render::{BlendMode, WindowCanvas};
use sdl2::video::{Window, WindowBuildError};

pub struct Screens {
    pub sdl: sdl2::Sdl,
    _video: sdl2::VideoSubsystem,
    /// One canvas spanning both screens, or one for each; `top` and `bot` say which is where.
    pub canvases: Vec<WindowCanvas>,
    pub top: View,
    pub bot: View,
    /// The two panels, as against the desktop preview.
    pub dual: bool,
    /// SDL's dummy video driver: drawn in software, shown nowhere (`--shot`).
    pub headless: bool,
}

pub fn open(opts: &Opts) -> Result<Screens, String> {
    // Let SDL take the outputs, while what DSlot put up (the skeleton, the shelf's last frame)
    // stays until the shelf's first frame.
    if let Some(outputs) = opts
        .outputs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        outputs.release();
    }
    // SDL3's atomic KMSDRM path sets up a plane only for the first window's display and
    // crashes presenting on the second; the legacy path drives both.
    if std::env::var_os("SDL_KMSDRM_ATOMIC").is_none() {
        std::env::set_var("SDL_KMSDRM_ATOMIC", "0");
    }
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    crate::mark("video ready");

    // Where the two panels are among SDL's displays (nibkit::screens): a display each, or one
    // spanning both. Neither: a desktop preview.
    let bounds: Vec<screens::Rect> = (0..video.num_video_displays()?)
        .filter_map(|i| video.display_bounds(i).ok())
        .map(|r| screens::Rect::new(r.x(), r.y(), r.width(), r.height()))
        .collect();
    let placement = screens::place(&bounds, opts.swap);
    let dual = placement.is_some();
    let headless = std::env::var("SDL_VIDEODRIVER").is_ok_and(|v| v == "dummy");

    let (windows, (top, bot)) = match placement {
        Some(p) if p.shared => shared(&video, &p),
        Some(p) => per_screen(&video, &p),
        // Headless, two windows as on nibOS, to catch a texture drawn on the wrong one.
        None if headless && std::env::var_os("DSLOT_TWO_WINDOWS").is_some() => two(&video),
        None => preview(&video, headless),
    };
    // Covers are drawn small in the grid; smooth them rather than drop pixels.
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "1");
    let mut canvases = Vec::new();
    for window in windows {
        let builder = window.map_err(|e| e.to_string())?.into_canvas();
        let builder = if headless {
            builder.software()
        } else {
            builder.accelerated().present_vsync()
        };
        let mut canvas = builder.build().map_err(|e| e.to_string())?;
        canvas.set_blend_mode(BlendMode::Blend);
        canvases.push(canvas);
    }
    crate::mark("window and renderer ready");
    sdl.mouse().show_cursor(!dual);
    Ok(Screens {
        sdl,
        _video: video,
        canvases,
        top,
        bot,
        dual,
        headless,
    })
}

type Windows = Vec<Result<Window, WindowBuildError>>;

/// Where a screen goes on a window whose top left is at `origin`: the 1024 x 768 layout scaled
/// to fit `r`, centred in it.
fn view(r: screens::Rect, origin: (i32, i32), c: usize) -> View {
    let s = (r.h as f32 / SH).min(r.w as f32 / SW);
    let ox = (r.x - origin.0) as f32 + (r.w as f32 - SW * s) / 2.0;
    let oy = (r.y - origin.1) as f32 + (r.h as f32 - SH * s) / 2.0;
    View { ox, oy, s, c }
}

/// Both screens on one display (Weston on the stock OS): one window across it, like the stock
/// emulator's.
fn shared(video: &sdl2::VideoSubsystem, p: &Placement) -> (Windows, (View, View)) {
    let (t, b) = (p.top.rect, p.bottom.rect);
    let (x, y) = (t.x.min(b.x), t.y.min(b.y));
    let (w, h) = (t.w + b.w, t.h.max(b.h));
    let window = video
        .window("DSlot", w, h)
        .position(x, y)
        .borderless()
        .build();
    (vec![window], (view(t, (x, y), 0), view(b, (x, y), 0)))
}

/// A display for each panel (KMS on nibOS: a CRTC each); one window can't span them, so each
/// screen gets a window and renderer of its own.
fn per_screen(video: &sdl2::VideoSubsystem, p: &Placement) -> (Windows, (View, View)) {
    let mut windows = Vec::new();
    let mut views = Vec::new();
    for (c, region) in [p.top, p.bottom].into_iter().enumerate() {
        let r = region.rect;
        views.push(view(r, (r.x, r.y), c));
        // OpenGL from the start: KMSDRM otherwise tears the window down and makes it again
        // when the GLES renderer is attached.
        let mut window = video.window("DSlot", r.w, r.h);
        windows.push(
            window
                .position(r.x, r.y)
                .fullscreen_desktop()
                .opengl()
                .build(),
        );
    }
    (windows, (views[0], views[1]))
}

/// A window for each screen, full size: nibOS's arrangement, headless (`DSLOT_TWO_WINDOWS`).
fn two(video: &sdl2::VideoSubsystem) -> (Windows, (View, View)) {
    let view = |c| View {
        ox: 0.0,
        oy: 0.0,
        s: 1.0,
        c,
    };
    let windows = (0..2)
        .map(|_| video.window("DSlot", SW as u32, SH as u32).build())
        .collect();
    (windows, (view(0), view(1)))
}

/// A desktop preview: the two screens stacked, as you hold the device, at half size (full size
/// when headless, for `--shot`).
fn preview(video: &sdl2::VideoSubsystem, headless: bool) -> (Windows, (View, View)) {
    let s = if headless { 1.0 } else { 0.5 };
    let window = video
        .window("DSlot", (SW * s) as u32, (SH * 2.0 * s) as u32)
        .position_centered()
        .build();
    (
        vec![window],
        (
            View {
                ox: 0.0,
                oy: 0.0,
                s,
                c: 0,
            },
            View {
                ox: 0.0,
                oy: SH * s,
                s,
                c: 0,
            },
        ),
    )
}
