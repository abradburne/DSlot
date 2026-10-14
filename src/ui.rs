//! The shelf: box and details on the top screen, cart carousel and slot on the touch screen.

use crate::boot;
use crate::art::{Shell, CART_H, CART_W, CASE_SIZE};
use crate::loader::{Job, Loader, Made, Source};
use crate::gfx::{self, save_png, Fonts, Img};
use crate::rom::Game;
use crate::settings::{Case, Settings, Sort, View as Layout};
use crate::stats::{self, Stats};
use crate::touch::{Panel, Pointer};
use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::AudioSubsystem;
use sdl2::controller::Button;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Texture, TextureCreator, WindowCanvas};
use sdl2::video::WindowContext;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Every screen is laid out in these units, then scaled to the panel.
const SW: f32 = 1024.0;
const SH: f32 = 768.0;

const STEP: f32 = 330.0;
const CART_SIZE: f32 = 400.0;
const CART_Y: f32 = 318.0;
const BAR_H: f32 = 96.0;
const INSERT_SECS: f32 = 0.42;
const FADE_SECS: f32 = 0.2;
const EJECT_SECS: f32 = 0.28;
/// How deep the cart sits in the slot, and how much further a push takes it before it latches.
const SEAT: f32 = 560.0;
const PRESS: f32 = 16.0;
const SPRING_SECS: f32 = 0.06;

/// A game card going into and out of a DS slot, recorded on the real thing.
/// 48 kHz, 16-bit, mono. Each clip has two clicks: the push, then the latch (or the pop).
const INSERT_PCM: &[u8] = include_bytes!("../assets/insert.pcm");
const EJECT_PCM: &[u8] = include_bytes!("../assets/eject.pcm");
const CLIP_HZ: usize = 48_000;
/// What SDL's audio device is opened at, where SDL is what plays the clips.
const DEVICE_HZ: usize = 44_100;
/// Seconds into each clip at which the first click lands.
const CLICK_LEAD: f32 = 0.025;
/// Seconds from the push click to the latch click (insert) and to the pop (eject).
const LATCH_GAP: f32 = 0.228;
const POP_GAP: f32 = 0.262;
/// Silence played ahead of the eject clip. The game has only just let go of the sound card,
/// and a device that is still starting up swallows the first click.
const EJECT_WARMUP: f32 = 0.35;
/// How long the queue must have been empty before the device's own buffer has played out.
const DRAIN_SECS: f32 = 0.25;
const PULL_TO_INSERT: f32 = 140.0;
const ROW_STEP: f32 = 48.0;
/// Rows of the menu: two for the selected game, three for the library, four for dslot and the device.
const MENU: [&str; 9] = ["Favourite", "Rating", "View", "Sort by", "Show", "Game case", "Sounds", "Start at power-on", "Power off"];
const POWER_ROW: usize = 8;
/// The cover grid: columns, whole rows on screen, a cover's size, and the pitch between covers.
const COLS: usize = 5;
const GRID_ROWS: usize = 3;
const GRID_COVER: (f32, f32) = (180.0, 164.0);
const GRID_STEP: (f32, f32) = (196.0, 190.0);
const GRID_AT: (f32, f32) = (30.0, 62.0);
/// Grid covers kept as textures. They are small, so in most libraries that is all of them.
const THUMBS: usize = 600;
/// Stars and hearts are drawn this size.
const ICON: f32 = 26.0;
const HINT: &str = "A  insert   \u{b7}   D-pad  browse   \u{b7}   X  menu   \u{b7}   Y  favourite   \u{b7}   Select + Start  exit";
const TAP_SLOP: f32 = 14.0;
const REPEAT_FIRST: Duration = Duration::from_millis(340);
const REPEAT_NEXT: Duration = Duration::from_millis(85);

const DIM: Color = Color::RGB(154, 163, 184);
const FAINT: Color = Color::RGB(98, 106, 128);
const WHITE: Color = Color::RGB(241, 243, 248);
const GOLD: Color = Color::RGB(255, 196, 60);
const RED: Color = Color::RGB(236, 72, 92);
const PANEL: Color = Color::RGB(28, 33, 44);

pub enum Outcome {
    Launch(usize),
    Quit,
    PowerOff,
}

pub struct Opts {
    pub start: usize,
    /// Coming back from a game: the cart pops out of the slot.
    pub returning: bool,
    /// Folder of numbered sound sets (1/, 2/, ...) to compare with the number keys.
    pub sounds: Option<PathBuf>,
    pub swap: bool,
    pub shot: Option<PathBuf>,
    /// Keys pressed before the shot is taken, by SDL name.
    pub press: Vec<String>,
    pub searched: Vec<PathBuf>,
    /// Folder for grid covers already scaled, so they are only made once.
    pub cache: PathBuf,
    /// The script that starts dslot at power-on, once installed where the stock boot script looks.
    pub boot_script: PathBuf,
}

#[derive(Clone, Copy)]
struct View {
    ox: f32,
    oy: f32,
    s: f32,
}

impl View {
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let (px, py) = ((self.ox + x * self.s).round() as i32, (self.oy + y * self.s).round() as i32);
        Rect::new(px, py, (w * self.s).round().max(1.0) as u32, (h * self.s).round().max(1.0) as u32)
    }
}

struct Art {
    cart: Texture,
    cover: Texture,
}

struct Drag {
    start: (f32, f32),
    at: (f32, f32),
    p0: f32,
    /// Grid scroll when the drag began.
    s0: f32,
    on_cart: bool,
}

struct Ui<'a> {
    canvas: WindowCanvas,
    tc: TextureCreator<WindowContext>,
    fonts: &'a Fonts,
    games: &'a [Game],
    case: Case,
    loader: Loader,
    top: View,
    bot: View,
    arts: HashMap<usize, Art>,
    /// Grid covers, by game.
    thumbs: HashMap<usize, Texture>,
    texts: HashMap<(String, u32, bool), (Texture, u32, u32)>,
    white: Texture,
    bar: Texture,
    star: Texture,
    heart: Texture,
}

pub fn run(games: &[Game], stats: &mut Stats, settings: &mut Settings, fonts: &Fonts, shell: &Shell, opts: &Opts) -> Result<Outcome, String> {
    // The artwork workers live exactly as long as the shelf is up.
    std::thread::scope(|scope| shelf(scope, games, stats, settings, fonts, shell, opts))
}

fn shelf<'scope, 'env: 'scope>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    games: &'env [Game],
    stats: &mut Stats,
    settings: &mut Settings,
    fonts: &'env Fonts,
    shell: &'env Shell,
    opts: &'env Opts,
) -> Result<Outcome, String> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    crate::mark("video ready");

    // The DS+ exposes its two panels as two displays side by side. Span both, like the stock emulator.
    let bounds: Vec<Rect> = (0..video.num_video_displays()?).filter_map(|i| video.display_bounds(i).ok()).collect();
    let union = bounds.iter().skip(1).fold(bounds[0], |a, b| a.union(*b));
    let dual = union.width() as f32 / union.height() as f32 > 2.2;
    let headless = std::env::var("SDL_VIDEODRIVER").is_ok_and(|v| v == "dummy");

    let (window, top, bot) = if dual {
        let (half, s) = (union.width() as f32 / 2.0, (union.height() as f32 / SH).min(union.width() as f32 / 2.0 / SW));
        let pad = ((half - SW * s) / 2.0, (union.height() as f32 - SH * s) / 2.0);
        let (left, right) = (View { ox: pad.0, oy: pad.1, s }, View { ox: half + pad.0, oy: pad.1, s });
        let w = video.window("DSlot", union.width(), union.height()).position(union.x(), union.y()).borderless().build();
        // The touch panel is the right-hand output, so that is the bottom screen.
        if opts.swap { (w, right, left) } else { (w, left, right) }
    } else {
        // Desktop preview: the two screens stacked, as you hold the device.
        let s = if headless { 1.0 } else { 0.5 };
        let w = video.window("DSlot", (SW * s) as u32, (SH * 2.0 * s) as u32).position_centered().build();
        (w, View { ox: 0.0, oy: 0.0, s }, View { ox: 0.0, oy: SH * s, s })
    };
    let window = window.map_err(|e| e.to_string())?;
    let builder = window.into_canvas();
    let builder = if headless { builder.software() } else { builder.accelerated().present_vsync() };
    // Covers are drawn small in the grid; smooth them rather than drop pixels.
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "1");
    let mut canvas = builder.build().map_err(|e| e.to_string())?;
    canvas.set_blend_mode(BlendMode::Blend);
    crate::mark("window and renderer ready");
    sdl.mouse().show_cursor(!dual);

    let pads = sdl.game_controller()?;
    for db in ["/mnt/vendor/sbin/gamecontrollerdb.txt", "/mnt/vendor/deep/nnddss/assets/gamecontrollerdb.txt"] {
        let _ = pads.load_mappings(db);
    }
    let _open: Vec<_> = (0..pads.num_joysticks()?).filter(|&i| pads.is_game_controller(i)).filter_map(|i| pads.open(i).ok()).collect();
    crate::mark("controllers ready");
    let mut sound = Sound { pacat: true, system: sdl.audio().ok(), queue: None, idle: 0.0, on: settings.sound };
    crate::mark("audio ready");
    let mut panel = if dual { Panel::open() } else { None };
    crate::mark("touch panel ready");

    let tc = canvas.texture_creator();
    let white = texture(&tc, &Img { w: 1, h: 1, px: vec![255; 4] })?;
    let bar = texture(&tc, &slot_bar(bot.s))?;
    let icon = (ICON * bot.s).round() as usize;
    let (star, heart) = (texture(&tc, &gfx::star(icon))?, texture(&tc, &gfx::heart(icon))?);
    let thumb = ((GRID_COVER.0 * bot.s).round() as usize, (GRID_COVER.1 * bot.s).round() as usize);
    let mut ui = Ui {
        canvas, tc, fonts, games, top, bot, white, bar, star, heart,
        case: settings.case,
        loader: Loader::start(scope, Source { games, fonts, shell, thumb, cache: &opts.cache }),
        arts: HashMap::new(),
        thumbs: HashMap::new(),
        texts: HashMap::new(),
    };

    // The games on show, in order, as indices into `games`. `sel` and `p` are places in this list.
    let mut order = library(games, stats, settings);
    let mut sel = order.iter().position(|&g| opts.returning && g == opts.start).unwrap_or(0);
    let mut scroll = (sel / COLS).saturating_sub(1) as f32;
    let mut p = sel as f32;
    let mut insert: Option<f32> = None;
    let mut clicked = false;
    let sounds: Vec<(Vec<u8>, Vec<u8>)> = opts.sounds.iter().flat_map(|dir| {
        (1..=4).map_while(move |i| {
            let set = dir.join(i.to_string());
            Some((std::fs::read(set.join("insert.pcm")).ok()?, std::fs::read(set.join("eject.pcm")).ok()?))
        })
    }).collect();
    let mut variant: Option<usize> = None;
    let mut eject: Option<f32> = (opts.returning && !order.is_empty()).then_some(-EJECT_WARMUP);
    let mut popped = false;
    let mut drag: Option<Drag> = None;
    let mut held: Option<(i32, Instant)> = None;
    let mut back_held = false;
    // The settings page, when open, and its highlighted row.
    let mut menu: Option<usize> = None;
    // Whether dslot starts at power-on; `None` where there is no way to arrange it.
    let mut at_boot = boot::available().then(|| boot::enabled(&opts.boot_script));
    // Power off has been asked for once and wants confirming.
    let mut armed = false;
    let mut events = sdl.event_pump()?;
    crate::mark("shelf set up");
    let mut pointers = Vec::new();
    let mut tick = Instant::now();
    let mut frame = 0u32;
    loop {
        let dt = tick.elapsed().as_secs_f32().min(0.05);
        tick = Instant::now();
        // One scripted key a frame, as a person would press them.
        if let Some(name) = opts.press.get(frame as usize) {
            let key = Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Keycode::from_name(name),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            };
            sdl.event()?.push_event(key)?;
        }
        let (n, last) = (order.len(), order.len().saturating_sub(1));
        let grid = settings.view == Layout::Grid;
        let busy = insert.is_some() || eject.is_some();
        let step = |sel: usize, d: i32| (sel as i32 + d).clamp(0, last as i32) as usize;
        // Up and down: a row in the grid, a letter of the alphabet on the shelf.
        let vert = |sel: usize, d: i32| if grid { step(sel, d * COLS as i32) } else { letter_jump(games, &order, sel, d) };
        let page = if grid { (COLS * GRID_ROWS) as i32 } else { 5 };
        // The list needs rebuilding: a favourite, a rating, the sort or the filter changed.
        let (mut fave, mut relist) = (false, false);

        pointers.clear();
        if let Some(panel) = panel.as_mut() {
            panel.poll(&mut pointers);
            for ptr in pointers.iter_mut() {
                *ptr = match *ptr {
                    Pointer::Down(x, y) => Pointer::Down(x * SW, y * SH),
                    Pointer::Move(x, y) => Pointer::Move(x * SW, y * SH),
                    Pointer::Up => Pointer::Up,
                };
            }
        }
        // Settings input this frame: rows to move by, steps to change the value by, and leave.
        // `pressed` is A or a tap, as against the D-pad: only those may power off.
        let (mut rows, mut turn, mut close, mut pressed) = (0i32, 0i32, false, false);
        let to_bot = |x: i32, y: i32| ((x as f32 - bot.ox) / bot.s, (y as f32 - bot.oy) / bot.s);

        for event in events.poll_iter() {
            match event {
                Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => return Ok(Outcome::Quit),
                Event::ControllerButtonDown { button: Button::Guide, .. } => return Ok(Outcome::Quit),
                Event::ControllerButtonDown { button: Button::Back, .. } => back_held = true,
                Event::ControllerButtonUp { button: Button::Back, .. } => back_held = false,
                Event::ControllerButtonDown { button: Button::Start, .. } if back_held => return Ok(Outcome::Quit),
                Event::ControllerButtonUp { button: Button::DPadLeft | Button::DPadRight, .. } => held = None,
                _ if busy => {}
                Event::KeyDown { keycode: Some(key), .. } if menu.is_some() => match key {
                    Keycode::Up => rows -= 1,
                    Keycode::Down => rows += 1,
                    Keycode::Left => turn -= 1,
                    Keycode::Right => turn += 1,
                    Keycode::Return | Keycode::Space => (turn, pressed) = (turn + 1, true),
                    Keycode::Tab | Keycode::Backspace => close = true,
                    _ => {}
                },
                Event::ControllerButtonDown { button, .. } if menu.is_some() => match button {
                    Button::DPadUp => rows -= 1,
                    Button::DPadDown => rows += 1,
                    Button::DPadLeft => turn -= 1,
                    Button::DPadRight => turn += 1,
                    // As with X and Y, the DS+ reports its A button as SDL's B and its B as A.
                    Button::B => (turn, pressed) = (turn + 1, true),
                    Button::A | Button::X | Button::Y | Button::Start => close = true,
                    _ => {}
                },
                // The DS+ reports its X button as SDL's Y and its Y as X.
                Event::KeyDown { keycode: Some(Keycode::Tab), .. } | Event::ControllerButtonDown { button: Button::Y, .. }
                    if !games.is_empty() =>
                {
                    menu = Some(0)
                }
                Event::KeyDown { .. } | Event::ControllerButtonDown { .. } if n == 0 => {}
                Event::KeyDown { keycode: Some(key), .. } => match key {
                    Keycode::F => fave = true,
                    Keycode::Left => sel = step(sel, -1),
                    Keycode::Right => sel = step(sel, 1),
                    Keycode::Up => sel = vert(sel, -1),
                    Keycode::Down => sel = vert(sel, 1),
                    Keycode::Return | Keycode::Space => insert = Some(0.0),
                    Keycode::Num1 | Keycode::Num2 | Keycode::Num3 | Keycode::Num4 => {
                        let keys = [Keycode::Num1, Keycode::Num2, Keycode::Num3, Keycode::Num4];
                        let v = keys.iter().position(|k| *k == key).unwrap_or(0);
                        if v < sounds.len() {
                            variant = Some(v);
                            sound.play(&sounds[v].0);
                        }
                    }
                    _ => {}
                },
                Event::ControllerButtonDown { button, .. } => match button {
                    Button::DPadLeft => (sel, held) = (step(sel, -1), Some((-1, Instant::now() + REPEAT_FIRST))),
                    Button::DPadRight => (sel, held) = (step(sel, 1), Some((1, Instant::now() + REPEAT_FIRST))),
                    Button::DPadUp => sel = vert(sel, -1),
                    Button::DPadDown => sel = vert(sel, 1),
                    Button::LeftShoulder => sel = step(sel, -page),
                    Button::RightShoulder => sel = step(sel, page),
                    Button::A | Button::B | Button::Start => insert = Some(0.0),
                    Button::X => fave = true,
                    _ => {}
                },
                // With the panel read directly, SDL's copies of the same touches are ignored.
                Event::MouseButtonDown { x, y, .. } if panel.is_none() => {
                    let (x, y) = to_bot(x, y);
                    if (0.0..SW).contains(&x) && (0.0..SH).contains(&y) {
                        pointers.push(Pointer::Down(x, y));
                    }
                }
                Event::MouseMotion { x, y, .. } if panel.is_none() => {
                    let (x, y) = to_bot(x, y);
                    pointers.push(Pointer::Move(x, y));
                }
                Event::MouseButtonUp { .. } if panel.is_none() => pointers.push(Pointer::Up),
                _ => {}
            }
        }

        if let Some((dir, at)) = held {
            if !busy && Instant::now() >= at {
                sel = step(sel, dir);
                held = Some((dir, Instant::now() + REPEAT_NEXT));
            }
        }

        let current = order.get(sel).map(|&g| &games[g]);
        if let (true, Some(game)) = (fave, current) {
            stats.toggle_fave(&game.path);
            relist = true;
        }
        if let Some(row) = menu.as_mut() {
            (held, drag) = (None, None);
            // A tap on a row steps its value.
            for ptr in pointers.drain(..) {
                if let Pointer::Down(_, y) = ptr {
                    if let Some(r) = (0..MENU.len()).find(|&r| (y - row_y(r)).abs() < ROW_STEP / 2.0) {
                        (*row, turn, pressed) = (r, turn + 1, true);
                    }
                }
            }
            let was = *row;
            // Up from the top row arrives at the bottom one, and the other way round.
            *row = (*row as i32 + rows).rem_euclid(MENU.len() as i32) as usize;
            if *row == POWER_ROW && pressed && was == *row {
                if armed {
                    return Ok(Outcome::PowerOff);
                }
                armed = true;
            } else if turn != 0 || was != *row {
                armed = false;
            }
            if turn != 0 && *row != POWER_ROW {
                let dir = turn.signum();
                match (*row, current) {
                    (0, Some(game)) => stats.toggle_fave(&game.path),
                    (1, Some(game)) => stats.set_rating(&game.path, (stats.rating(&game.path) as i32 + dir).rem_euclid(6) as u8),
                    (0 | 1, None) => {}
                    (2, _) => settings.view = settings.view.step(dir),
                    (3, _) => settings.sort = settings.sort.step(dir),
                    (4, _) => settings.faves_only = !settings.faves_only,
                    (5, _) => settings.case = settings.case.step(dir),
                    (6, _) => settings.sound = !settings.sound,
                    _ => {
                        if let Some(on) = at_boot {
                            if let Err(e) = boot::set(!on, &opts.boot_script) {
                                eprintln!("dslot: start at power-on: {e}");
                            }
                            at_boot = Some(boot::enabled(&opts.boot_script));
                        }
                    }
                }
                settings.save();
                sound.on = settings.sound;
                if ui.case != settings.case {
                    ui.case = settings.case;
                    ui.forget_art();
                }
                relist = true;
            }
            if close {
                (menu, armed) = (None, false);
            }
        }
        if relist {
            // Stay on the same game where it is still listed.
            let was = order.get(sel).copied();
            order = library(games, stats, settings);
            sel = was.and_then(|g| order.iter().position(|&o| o == g)).unwrap_or(sel.min(order.len().saturating_sub(1)));
            p = sel as f32;
            (held, drag) = (None, None);
            pointers.clear();
        }

        for ptr in pointers.iter().filter(|_| !busy && n > 0) {
            match *ptr {
                Pointer::Down(x, y) => {
                    let on_cart = !grid && (x - SW / 2.0).abs() < CART_SIZE / 2.0 && (y - CART_Y).abs() < CART_SIZE / 2.0;
                    drag = Some(Drag { start: (x, y), at: (x, y), p0: p, s0: scroll, on_cart });
                }
                Pointer::Move(x, y) => {
                    if let Some(d) = drag.as_mut() {
                        d.at = (x, y);
                    }
                }
                Pointer::Up => {
                    if let Some(d) = drag.take() {
                        let (dx, dy) = (d.at.0 - d.start.0, d.at.1 - d.start.1);
                        if grid && dx.abs() < TAP_SLOP && dy.abs() < TAP_SLOP {
                            // A tap picks a cover; a tap on the one already picked plays it.
                            match grid_hit(d.start, scroll).filter(|&i| i < n) {
                                Some(i) if i == sel => insert = Some(0.0),
                                Some(i) => sel = i,
                                None => {}
                            }
                        } else if grid {
                            // After a scroll, bring the selection onto the rows now in view.
                            let top = scroll.round().max(0.0) as usize;
                            let row = (sel / COLS).clamp(top, top + GRID_ROWS - 1);
                            sel = (row * COLS + sel % COLS).min(last);
                        } else if d.on_cart && dy > PULL_TO_INSERT && dy > dx.abs() {
                            insert = Some(0.0);
                        } else if dx.abs() < TAP_SLOP && dy.abs() < TAP_SLOP {
                            // A tap on a neighbouring cart brings it to the centre.
                            let side = ((d.start.0 - SW / 2.0) / STEP).round() as i32;
                            sel = step(sel, side);
                        } else {
                            sel = p.round().clamp(0.0, last as f32) as usize;
                        }
                    }
                }
            }
        }

        // Where the carousel is, and how far the centre cart has been pulled toward the slot.
        let mut pull = 0.0;
        match if grid { &None } else { &drag } {
            Some(d) if d.on_cart && d.at.1 - d.start.1 > (d.at.0 - d.start.0).abs() => pull = d.at.1 - d.start.1,
            Some(d) if (d.at.0 - d.start.0).abs() > TAP_SLOP => {
                p = (d.p0 - (d.at.0 - d.start.0) / STEP).clamp(-0.3, last as f32 + 0.3);
                sel = p.round().clamp(0.0, last as f32) as usize;
            }
            Some(_) => {}
            None => p += (sel as f32 - p) * (1.0 - (-dt * 14.0).exp()),
        }
        if grid {
            let most = n.div_ceil(COLS).saturating_sub(GRID_ROWS) as f32;
            match &drag {
                Some(d) if (d.at.1 - d.start.1).abs() > TAP_SLOP => {
                    scroll = (d.s0 - (d.at.1 - d.start.1) / GRID_STEP.1).clamp(-0.3, most + 0.3);
                }
                Some(_) => {}
                // Scroll just far enough to keep the selected row in view.
                None => {
                    let row = (sel / COLS) as f32;
                    let to = scroll.clamp(row - (GRID_ROWS - 1) as f32, row).clamp(0.0, most);
                    scroll += (to - scroll) * (1.0 - (-dt * 14.0).exp());
                }
            }
        }
        if let Some(t) = insert.as_mut() {
            *t += dt;
            // Slide in and push past the seat; the cart springs back as the latch clicks.
            let latch = ((*t - INSERT_SECS - LATCH_GAP) / SPRING_SECS).clamp(0.0, 1.0);
            pull = (*t / INSERT_SECS).min(1.0).powi(2) * (SEAT + PRESS) - PRESS * latch;
            // Queue the clip as the cart starts to move, behind enough silence that its first click
            // lands as the cart bottoms out. The silence gives the sound card time to wake up.
            if !clicked {
                sound.play_after(INSERT_SECS - CLICK_LEAD, variant.map_or(INSERT_PCM, |v| &sounds[v].0));
                clicked = true;
            }
            // Leaving closes the audio device, so let the latch click finish first.
            if *t > INSERT_SECS + LATCH_GAP + FADE_SECS && sound.drained() {
                return Ok(Outcome::Launch(order[sel]));
            }
        }
        if let Some(t) = eject {
            if !popped {
                sound.play_after(EJECT_WARMUP, variant.map_or(EJECT_PCM, |v| &sounds[v].1));
                popped = true;
            }
            // Pushed in on the first click, then thrown clear on the second.
            let pop = CLICK_LEAD + POP_GAP;
            let push = ((t - CLICK_LEAD) / SPRING_SECS).clamp(0.0, 1.0);
            let out = ((t - pop) / EJECT_SECS).clamp(0.0, 1.0);
            pull = (SEAT + PRESS * push) * (1.0 - out).powi(2);
            eject = (t < pop + EJECT_SECS).then_some(t + dt.max(0.001));
        }
        sound.tick(dt);
        let fade = insert.map_or(0.0, |t| ((t - INSERT_SECS - LATCH_GAP) / FADE_SECS).clamp(0.0, 1.0));

        ui.canvas.set_clip_rect(None);
        ui.canvas.set_draw_color(Color::RGB(0, 0, 0));
        ui.canvas.clear();
        let mut settled = false;
        if games.is_empty() {
            ui.empty(&opts.searched)?;
        } else {
            let n = order.len();
            // Ask for artwork in the order it is wanted: the selection, what else is on screen,
            // then what is next to it. Every grid cover follows, nearest first, so that scrolling
            // finds them already made.
            let near: Vec<usize> = if grid {
                let top = scroll.max(0.0) as usize * COLS;
                (top..top + COLS * (GRID_ROWS + 1)).collect()
            } else {
                (1..=3).flat_map(|d| [sel.wrapping_sub(d), sel + d]).collect()
            };
            let next: Vec<usize> = if grid {
                vec![sel.wrapping_sub(1), sel + 1, sel.wrapping_sub(COLS), sel + COLS]
            } else {
                (4..=8).flat_map(|d| [sel.wrapping_sub(d), sel + d]).collect()
            };
            let kind = |i: &usize| if grid { Job::Thumb(*i) } else { Job::Art(*i) };
            let wants = [Job::Art(sel), Job::Thumb(sel)].into_iter()
                .chain(near.iter().map(kind))
                .chain(next.iter().map(|&i| Job::Art(i)))
                .chain((1..=THUMBS / 2).flat_map(|d| [Job::Thumb(sel.wrapping_sub(d)), Job::Thumb(sel + d)]));
            let hold: HashSet<usize> = if grid { next.iter().copied().chain([sel]).collect() } else { near.iter().chain(&next).copied().chain([sel]).collect() };
            settled = ui.load(&order, wants, &hold, sel)?;
            if frame == 0 {
                crate::mark("artwork asked for");
            }

            let place = (n > 0).then(|| format!("{} / {n}", sel + 1));
            let notes = [settings.faves_only.then_some("Favourites"), (settings.sort != Sort::Name).then_some(settings.sort.label())];
            let heading: Vec<String> = place.into_iter().chain(notes.into_iter().flatten().map(str::to_owned)).collect();
            let heading = heading.join("   \u{b7}   ");
            let current = order.get(sel).copied();
            match current {
                Some(g) => ui.draw_top(g, stats, fade)?,
                None => {
                    ui.text(top, "No favourites yet", 48.0, true, SW / 2.0, 320.0, 0.5, WHITE)?;
                    ui.text(top, "Show all games again from the menu.", 24.0, false, SW / 2.0, 392.0, 0.5, DIM)?;
                }
            }
            match menu {
                Some(row) => ui.draw_menu(row, settings, stats, current, at_boot, armed)?,
                None if grid => ui.draw_grid(&order, sel, scroll, stats, &heading, fade)?,
                None => ui.draw_bottom(&order, sel, p, pull, &heading, fade)?,
            }
            if let Some(v) = variant {
                ui.text(bot, &format!("Sound {} of {}", v + 1, sounds.len()), 24.0, true, 40.0, 22.0, 0.0, WHITE)?;
            }
        }
        if frame == 0 {
            crate::mark("first frame drawn");
        }
        ui.canvas.present();
        if frame < 2 {
            crate::mark(if frame == 0 { "first frame shown" } else { "second frame shown" });
        }
        if frame == 1 {
            crate::gfx::warm_fallback();
            // Started early by dsboot, which holds back the radios until this is up.
            if let Ok(shown) = std::env::var("DSBOOT_SHOWN") {
                let _ = std::fs::write(shown, b"");
            }
        }
        frame += 1;

        if let Some(path) = &opts.shot {
            // Not before the artwork has arrived.
            let due = opts.press.len() + if opts.press.is_empty() { 2 } else { 40 };
            if frame as usize >= due && (settled || games.is_empty()) {
                let (w, h) = ui.canvas.output_size()?;
                let px = ui.canvas.read_pixels(None, PixelFormatEnum::ABGR8888)?;
                save_png(path, w, h, &px)?;
                return Ok(Outcome::Quit);
            }
        }
        if headless {
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

/// On the DS+ sound goes through PulseAudio, and SDL only reaches that by way of an ALSA
/// shim whose small buffers underrun on every period, each one a gap in the click. So a clip
/// is handed whole to `pacat`, PulseAudio's own player, which also lets it finish after the
/// shelf has gone. Without `pacat` (a desktop), SDL plays it: the device is opened for each
/// click and closed again once it has played.
struct Sound {
    /// Cleared the first time `pacat` cannot be started.
    pacat: bool,
    system: Option<AudioSubsystem>,
    queue: Option<AudioQueue<i16>>,
    idle: f32,
    on: bool,
}

impl Sound {
    /// Sound is optional: with no audio device the shelf is simply silent.
    fn play(&mut self, pcm: &[u8]) {
        self.play_after(0.0, pcm)
    }

    /// Plays `pcm` behind `lead` seconds of silence.
    fn play_after(&mut self, lead: f32, pcm: &[u8]) {
        if !self.on {
            return;
        }
        if self.pacat {
            match pacat(lead, pcm) {
                Ok(()) => return,
                Err(_) => self.pacat = false,
            }
        }
        if self.queue.is_none() {
            let spec = AudioSpecDesired { freq: Some(DEVICE_HZ as i32), channels: Some(2), samples: Some(2048) };
            self.queue = self.system.as_ref().and_then(|a| a.open_queue::<i16, _>(None, &spec).ok());
        }
        if let Some(queue) = &self.queue {
            let clip: Vec<f32> = pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32).collect();
            let mut samples = vec![0i16; (lead * DEVICE_HZ as f32) as usize * 2];
            // Resampled to the device's rate, the same on both channels.
            for i in 0..clip.len() * DEVICE_HZ / CLIP_HZ {
                let at = i as f32 * CLIP_HZ as f32 / DEVICE_HZ as f32;
                let (a, b) = (clip[at as usize], clip[(at as usize + 1).min(clip.len() - 1)]);
                samples.extend([(a + (b - a) * at.fract()) as i16; 2]);
            }
            let _ = queue.queue_audio(&samples);
            queue.resume();
            self.idle = 0.0;
        }
    }

    /// True once everything queued has reached the speaker (or there is no sound at all).
    /// What `pacat` is playing needs no waiting for.
    fn drained(&self) -> bool {
        self.queue.as_ref().is_none_or(|q| q.size() == 0 && self.idle > DRAIN_SECS)
    }

    fn tick(&mut self, dt: f32) {
        if self.queue.as_ref().is_some_and(|q| q.size() == 0) {
            self.idle += dt;
            if self.idle > 0.4 {
                self.queue = None;
            }
        }
    }
}

/// Play 48 kHz mono `pcm` behind `lead` seconds of silence through PulseAudio's own player.
fn pacat(lead: f32, pcm: &[u8]) -> std::io::Result<()> {
    let rate = format!("--rate={CLIP_HZ}");
    let args = ["--playback", "--raw", "--format=s16le", "--channels=1", &rate, "--latency-msec=30", "--client-name=DSlot"];
    let mut child = Command::new("pacat").args(args).stdin(Stdio::piped()).stdout(Stdio::null()).spawn()?;
    let mut data = vec![0u8; (lead * CLIP_HZ as f32) as usize * 2];
    data.extend_from_slice(pcm);
    // Fed from its own thread: the clip is larger than a pipe holds. `pacat` plays out what
    // it was given and exits once its input is closed.
    std::thread::spawn(move || {
        if let Some(mut pipe) = child.stdin.take() {
            let _ = pipe.write_all(&data);
        }
        let _ = child.wait();
    });
    Ok(())
}

/// The games to show, in order: indices into `games`, which is itself sorted by name.
fn library(games: &[Game], stats: &Stats, settings: &Settings) -> Vec<usize> {
    let mut order: Vec<usize> = (0..games.len()).filter(|&i| !settings.faves_only || stats.fave(&games[i].path)).collect();
    // Both sorts are stable, so games that tie stay in name order.
    match settings.sort {
        Sort::Name => {}
        Sort::Recent => order.sort_by_key(|&i| std::cmp::Reverse(stats.last(&games[i].path))),
        Sort::Rating => order.sort_by_key(|&i| std::cmp::Reverse(stats.rating(&games[i].path))),
    }
    order
}

/// Next game in `order` whose name starts with a different letter, in direction `dir`.
fn letter_jump(games: &[Game], order: &[usize], sel: usize, dir: i32) -> usize {
    let initial = |i: usize| games[order[i]].name().chars().next().map(|c| c.to_ascii_lowercase());
    let mut i = sel as i32;
    loop {
        let next = i + dir;
        if next < 0 || next >= order.len() as i32 {
            return i as usize;
        }
        i = next;
        if initial(i as usize) != initial(sel) {
            return i as usize;
        }
    }
}

/// Top-left corner of a place in the cover grid.
fn grid_cell(place: usize, scroll: f32) -> (f32, f32) {
    (GRID_AT.0 + (place % COLS) as f32 * GRID_STEP.0, GRID_AT.1 + ((place / COLS) as f32 - scroll) * GRID_STEP.1)
}

/// The place in the cover grid under a touch, which may be past the last game.
fn grid_hit(at: (f32, f32), scroll: f32) -> Option<usize> {
    let (col, row) = ((at.0 - GRID_AT.0) / GRID_STEP.0, (at.1 - GRID_AT.1) / GRID_STEP.1 + scroll);
    (col >= 0.0 && (col as usize) < COLS && row >= 0.0).then(|| row as usize * COLS + col as usize)
}

fn texture(tc: &TextureCreator<WindowContext>, img: &Img) -> Result<Texture, String> {
    let mut t = tc.create_texture_static(PixelFormatEnum::ABGR8888, img.w as u32, img.h as u32).map_err(|e| e.to_string())?;
    t.update(None, &img.px, img.w * 4).map_err(|e| e.to_string())?;
    t.set_blend_mode(BlendMode::Blend);
    Ok(t)
}

/// The status bar with the slot recess cut out of its top edge.
fn slot_bar(s: f32) -> Img {
    let (w, h) = ((SW * s) as usize, (BAR_H * s) as usize);
    let (cx, rx, ry) = (w as f32 / 2.0, 215.0 * s, 54.0 * s);
    let mut img = Img::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx) / rx).hypot((y as f32 + 0.5) / ry);
            let a = (0.5 + (d - 1.0) * ry).clamp(0.0, 1.0);
            img.px[(y * w + x) * 4..][..4].copy_from_slice(&[28, 33, 44, (a * 255.0) as u8]);
        }
    }
    img
}

/// Where a row of the settings page sits on the touch screen.
fn row_y(row: usize) -> f32 {
    let gaps = (row >= 2) as usize + (row >= 5) as usize;
    112.0 + row as f32 * ROW_STEP + gaps as f32 * 40.0
}

impl Ui<'_> {
    /// Forget all artwork, to be composed again as it is wanted.
    fn forget_art(&mut self) {
        self.loader.forget();
        for (_, a) in self.arts.drain() {
            unsafe {
                a.cart.destroy();
                a.cover.destroy();
            }
        }
        for (_, t) in self.thumbs.drain() {
            unsafe { t.destroy() };
        }
    }

    /// Take in artwork the workers have finished and hand out more. `wants` and `hold` are
    /// places in `list`: what to make, most wanted first, and whose full-size artwork to keep.
    /// Returns whether everything wanted is here.
    fn load(&mut self, list: &[usize], wants: impl Iterator<Item = Job>, hold: &HashSet<usize>, sel: usize) -> Result<bool, String> {
        for (job, made) in self.loader.take() {
            match (job, made) {
                (Job::Art(g), Made::Art { cart, cover }) => {
                    self.arts.insert(g, Art { cart: texture(&self.tc, &cart)?, cover: texture(&self.tc, &cover)? });
                }
                (Job::Thumb(g), Made::Thumb(img)) => {
                    self.thumbs.insert(g, texture(&self.tc, &img)?);
                }
                _ => {}
            }
        }

        let held: HashSet<usize> = hold.iter().filter_map(|&i| list.get(i).copied()).collect();
        let gone: Vec<usize> = self.arts.keys().copied().filter(|g| !held.contains(g)).collect();
        for g in gone {
            if let Some(a) = self.arts.remove(&g) {
                unsafe {
                    a.cart.destroy();
                    a.cover.destroy();
                }
            }
        }
        if self.thumbs.len() > THUMBS {
            // A very large library: keep the covers around the selection.
            let around: HashSet<usize> = list.iter().skip(sel.saturating_sub(THUMBS / 2)).take(THUMBS).copied().collect();
            let gone: Vec<usize> = self.thumbs.keys().copied().filter(|g| !around.contains(g)).collect();
            for g in gone {
                if let Some(t) = self.thumbs.remove(&g) {
                    unsafe { t.destroy() };
                }
            }
        }

        let mut settled = self.loader.idle();
        for want in wants {
            // Places past either end of the list wrap round to nothing.
            let (place, have) = match want {
                Job::Art(i) => (i, list.get(i).map(|&g| (Job::Art(g), self.arts.contains_key(&g)))),
                Job::Thumb(i) => (i, list.get(i).map(|&g| (Job::Thumb(g), self.thumbs.contains_key(&g)))),
            };
            let Some((job, false)) = have else { continue };
            settled = false;
            if !self.loader.free() {
                break;
            }
            if !self.loader.asked(job) {
                let game = &self.games[list[place]];
                self.loader.ask(job, self.case.black(&game.code));
            }
        }
        Ok(settled)
    }

    /// Draw one line of text. `anchor` is 0 for left, 0.5 for centred, 1 for right aligned at `x`.
    fn text(&mut self, v: View, s: &str, size: f32, bold: bool, x: f32, y: f32, anchor: f32, c: Color) -> Result<(), String> {
        if s.is_empty() {
            return Ok(());
        }
        let px = (size * v.s).round() as u32;
        let key = (s.to_owned(), px, bold);
        if !self.texts.contains_key(&key) {
            if self.texts.len() > 160 {
                for (_, (t, _, _)) in self.texts.drain() {
                    unsafe { t.destroy() };
                }
            }
            let img = self.fonts.render(s, px as f32, bold);
            self.texts.insert(key.clone(), (texture(&self.tc, &img)?, img.w as u32, img.h as u32));
        }
        let (t, w, h) = self.texts.get_mut(&key).unwrap();
        t.set_color_mod(c.r, c.g, c.b);
        let dst = Rect::new((v.ox + x * v.s - anchor * *w as f32).round() as i32, (v.oy + y * v.s).round() as i32, *w, *h);
        self.canvas.copy(t, None, dst)
    }

    fn shade(&mut self, v: View, alpha: f32) -> Result<(), String> {
        if alpha <= 0.0 {
            return Ok(());
        }
        self.white.set_color_mod(0, 0, 0);
        self.white.set_alpha_mod((alpha * 255.0) as u8);
        self.canvas.copy(&self.white, None, v.rect(0.0, 0.0, SW, SH))
    }

    fn stars(&mut self, v: View, x: f32, y: f32, rating: u8) -> Result<(), String> {
        for i in 0..5 {
            let c = if i < rating { GOLD } else { Color::RGB(52, 58, 74) };
            self.star.set_color_mod(c.r, c.g, c.b);
            self.canvas.copy(&self.star, None, v.rect(x + i as f32 * (ICON + 4.0), y, ICON, ICON))?;
        }
        Ok(())
    }

    fn heart(&mut self, v: View, x: f32, y: f32, c: Color) -> Result<(), String> {
        self.heart.set_color_mod(c.r, c.g, c.b);
        self.canvas.copy(&self.heart, None, v.rect(x, y, ICON, ICON))
    }

    /// Hint line, status bar and the fade to black that goes with a launch.
    fn footer(&mut self, v: View, hint: &str, fade: f32) -> Result<(), String> {
        self.canvas.set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.text(v, hint, 18.0, false, SW / 2.0, 636.0, 0.5, FAINT)?;
        self.canvas.copy(&self.bar, None, v.rect(0.0, SH - BAR_H, SW, BAR_H))?;
        if let Some(b) = stats::battery() {
            self.text(v, &b, 24.0, true, 40.0, SH - 62.0, 0.0, DIM)?;
        }
        self.text(v, &stats::clock(), 24.0, true, SW - 40.0, SH - 62.0, 1.0, DIM)?;
        self.shade(v, fade)
    }

    fn draw_top(&mut self, sel: usize, stats: &Stats, fade: f32) -> Result<(), String> {
        let v = self.top;
        self.canvas.set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let game = &self.games[sel];

        // Until the full-size box is ready, its grid cover stands in.
        let cover = self.arts.get_mut(&sel).map(|a| &mut a.cover).or(self.thumbs.get_mut(&sel));
        if let Some(cover) = cover {
            let (w, h) = (CASE_SIZE.0 as f32, CASE_SIZE.1 as f32);
            let dst = v.rect(300.0 - w / 2.0, SH / 2.0 - h / 2.0, w, h);
            let mut shadow = dst;
            shadow.offset((10.0 * v.s) as i32, (14.0 * v.s) as i32);
            // The shadow is the box itself in black, so it has the case's rounded corners.
            cover.set_color_mod(0, 0, 0);
            cover.set_alpha_mod(120);
            self.canvas.copy_ex(&cover, None, shadow, -3.0, None, false, false)?;
            cover.set_color_mod(255, 255, 255);
            cover.set_alpha_mod(255);
            self.canvas.copy_ex(&cover, None, dst, -3.0, None, false, false)?;
        }

        let (x, width) = (580.0, 400.0);
        let size = self.fonts.fit(game.name(), 58.0, 28.0, width, true);
        let mut y = 228.0;
        if stats.fave(&game.path) {
            self.heart(v, x, y - 44.0, RED)?;
        }
        self.text(v, game.name(), size, true, x, y, 0.0, WHITE)?;
        y += size * 1.3;
        for line in game.title.iter().skip(1).take(2) {
            self.text(v, line, 26.0, false, x, y, 0.0, DIM)?;
            y += 34.0;
        }
        self.text(v, &game.serial(), 22.0, false, x, y + 4.0, 0.0, FAINT)?;

        let rows = [
            ("Played", stats.played(&game.path)),
            ("Last played", stats.last_played(&game.path)),
            ("Sessions", stats.sessions(&game.path)),
        ];
        let mut y = y.max(380.0) + 70.0;
        for (label, value) in rows {
            self.text(v, label, 22.0, false, x, y, 0.0, DIM)?;
            self.text(v, &value, 22.0, true, x + 150.0, y, 0.0, WHITE)?;
            y += 36.0;
        }
        self.text(v, "Rating", 22.0, false, x, y, 0.0, DIM)?;
        self.stars(v, x + 150.0, y + 2.0, stats.rating(&game.path))?;
        self.shade(v, fade)
    }

    fn draw_bottom(&mut self, list: &[usize], sel: usize, p: f32, pull: f32, heading: &str, fade: f32) -> Result<(), String> {
        let v = self.bot;
        self.canvas.set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.text(v, heading, 20.0, false, SW / 2.0, 24.0, 0.5, FAINT)?;

        // Furthest carts first, so the centre one sits on top.
        let shown = |i: &usize| (*i as f32 - p).abs() < 2.6 && self.arts.contains_key(&list[*i]);
        let mut order: Vec<usize> = (0..list.len()).filter(shown).collect();
        order.sort_by(|a, b| (*b as f32 - p).abs().total_cmp(&(*a as f32 - p).abs()));
        for i in order {
            let d = i as f32 - p;
            let away = d.abs().min(1.0);
            let w = CART_SIZE * (1.0 - 0.3 * away);
            let h = w * CART_H as f32 / CART_W as f32;
            let y = CART_Y + if i == sel { pull } else { 0.0 };
            let dst = v.rect(SW / 2.0 + d * STEP - w / 2.0, y - h / 2.0, w, h);
            let a = self.arts.get_mut(&list[i]).unwrap();
            a.cart.set_alpha_mod((255.0 - 150.0 * away) as u8);
            self.canvas.copy(&a.cart, None, dst)?;
        }

        self.footer(v, HINT, fade)
    }

    /// A wall of boxes in place of the carts.
    fn draw_grid(&mut self, list: &[usize], sel: usize, scroll: f32, stats: &Stats, heading: &str, fade: f32) -> Result<(), String> {
        let v = self.bot;
        self.canvas.set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.text(v, heading, 20.0, false, SW / 2.0, 24.0, 0.5, FAINT)?;
        if list.is_empty() {
            return self.footer(v, HINT, fade);
        }

        self.canvas.set_clip_rect(v.rect(0.0, GRID_AT.1 - 8.0, SW, GRID_ROWS as f32 * GRID_STEP.1 - 8.0));
        let first = scroll.max(0.0) as usize * COLS;
        for place in first..(first + COLS * (GRID_ROWS + 1)).min(list.len()) {
            let (x, y) = grid_cell(place, scroll);
            let (w, h) = GRID_COVER;
            self.white.set_alpha_mod(255);
            if place == sel {
                self.white.set_color_mod(WHITE.r, WHITE.g, WHITE.b);
                self.canvas.copy(&self.white, None, v.rect(x - 5.0, y - 5.0, w + 10.0, h + 10.0))?;
            }
            // The panel colour stands in for a cover that is not composed yet.
            self.white.set_color_mod(PANEL.r, PANEL.g, PANEL.b);
            self.canvas.copy(&self.white, None, v.rect(x, y, w, h))?;
            if let Some(t) = self.thumbs.get_mut(&list[place]) {
                t.set_color_mod(255, 255, 255);
                t.set_alpha_mod(if place == sel { 255 } else { 205 });
                self.canvas.copy(t, None, v.rect(x, y, w, h))?;
                t.set_alpha_mod(255);
            }
            if stats.fave(&self.games[list[place]].path) {
                self.heart(v, x + w - ICON - 9.0, y + 11.0, Color::RGB(0, 0, 0))?;
                self.heart(v, x + w - ICON - 10.0, y + 9.0, RED)?;
            }
        }
        self.footer(v, HINT, fade)
    }

    /// The menu takes the touch screen; the box above shows the result.
    fn draw_menu(&mut self, row: usize, settings: &Settings, stats: &Stats, game: Option<usize>, at_boot: Option<bool>, armed: bool) -> Result<(), String> {
        let v = self.bot;
        self.canvas.set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let path = game.map(|g| self.games[g].path.clone());
        let name = game.map_or("No game selected", |g| self.games[g].name()).to_owned();
        for (r, heading) in [(0, name.as_str()), (2, "Library"), (5, "DSlot")] {
            self.text(v, heading, 20.0, true, 150.0, row_y(r) - 54.0, 0.0, FAINT)?;
        }
        for (r, label) in MENU.iter().enumerate() {
            let y = row_y(r);
            if r == row {
                self.white.set_color_mod(PANEL.r, PANEL.g, PANEL.b);
                self.white.set_alpha_mod(255);
                self.canvas.copy(&self.white, None, v.rect(120.0, y - ROW_STEP / 2.0 + 2.0, SW - 240.0, ROW_STEP - 4.0))?;
            }
            self.text(v, label, 28.0, false, 150.0, y - 17.0, 0.0, if r == row { WHITE } else { DIM })?;
            let value = match (r, &path) {
                (1, Some(path)) => {
                    self.stars(v, SW - 150.0 - 5.0 * (ICON + 4.0) + 4.0, y - ICON / 2.0, stats.rating(path))?;
                    continue;
                }
                (0, Some(path)) => if stats.fave(path) { "Yes" } else { "No" },
                (0 | 1, None) => "\u{2013}",
                (2, _) => settings.view.label(),
                (3, _) => settings.sort.label(),
                (4, _) => if settings.faves_only { "Favourites" } else { "All games" },
                (5, _) => settings.case.label(),
                (6, _) => if settings.sound { "On" } else { "Off" },
                (7, _) => at_boot.map_or("\u{2013}", |on| if on { "On" } else { "Off" }),
                // Not a setting: A asks, and A again does it.
                _ => {
                    let ask = if armed { "Press A again to power off" } else if r == row { "Press A" } else { "" };
                    self.text(v, ask, 28.0, true, SW - 150.0, y - 17.0, 1.0, if armed { RED } else { WHITE })?;
                    continue;
                }
            };
            let value = if r == row { format!("\u{2039}   {value}   \u{203a}") } else { value.to_owned() };
            self.text(v, &value, 28.0, true, SW - 150.0, y - 17.0, 1.0, WHITE)?;
        }
        self.footer(v, "D-pad  change   \u{b7}   B  back", 0.0)
    }

    fn empty(&mut self, searched: &[PathBuf]) -> Result<(), String> {
        let (top, bot) = (self.top, self.bot);
        self.text(top, "No DS games found", 48.0, true, SW / 2.0, 320.0, 0.5, WHITE)?;
        self.text(top, "Copy .nds files to one of the folders below.", 24.0, false, SW / 2.0, 392.0, 0.5, DIM)?;
        let mut y = 300.0;
        for dir in searched {
            self.text(bot, &dir.to_string_lossy(), 24.0, false, SW / 2.0, y, 0.5, DIM)?;
            y += 38.0;
        }
        self.text(bot, "Select + Start  exit", 18.0, false, SW / 2.0, 636.0, 0.5, FAINT)
    }
}
