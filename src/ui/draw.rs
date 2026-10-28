//! What the shelf draws with, and how each part of the screens is drawn: the box and details
//! on the top screen, the carts or the cover grid, the tabs and the status bar on the touch
//! screen. Artwork is composed off this thread (loader.rs) and arrives here as images to make
//! into textures; text is rendered once per string and size and kept.

use super::bouncer::{self, Bouncer};
use super::cheevos::{Cheevos, ListView};
use super::layout::*;
use super::menu::{Shown, Value};
use super::power;
use crate::achievements::Ra;
use crate::art::{self, Shell, Slot};
use crate::device::About;
use crate::gfx::{self, Fonts, Img};
use crate::loader::{Job, Loader, Made, Source};
use crate::resume::{Resume, Wanted};
use crate::rom::{Console, Game};
use crate::stats::{self, Stats};
use crate::systems::Card;
use nibkit::menu::colour;
use nibkit::net::{Bluetooth, Wifi};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Texture, TextureCreator, WindowCanvas};
use sdl2::video::WindowContext;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::thread::Scope;
use std::time::Instant;

// nibkit's menu colours, so that the shelf and the menu are one palette.
pub const DIM: Color = rgb(colour::DIM);
pub const FAINT: Color = rgb(colour::FAINT);
pub const WHITE: Color = rgb(colour::WHITE);
pub const GOLD: Color = rgb(colour::GOLD);
pub const RED: Color = rgb(colour::RED);
pub const PANEL: Color = rgb(colour::PANEL);

const fn rgb([r, g, b]: colour::Rgb) -> Color {
    Color::RGB(r, g, b)
}

/// What the touch screen's hint line says on the shelf.
pub const HINT: &str = "A  insert  \u{b7}  D-pad  browse  \u{b7}  L / R  page  \u{b7}  Select  systems  \u{b7}  X  menu  \u{b7}  Y  favourite  \u{b7}  Select + Start  exit";
/// The same on a page without carts (Game & Watch, the apps), where there is none to insert.
const START_HINT: &str = "A  start  \u{b7}  D-pad  browse  \u{b7}  L / R  page  \u{b7}  Select  systems  \u{b7}  X  menu  \u{b7}  Y  favourite  \u{b7}  Select + Start  exit";
/// What the touch screen's hint line says over the system picker.
/// The stopped notice: how many characters fit a line of its box, and how many lines it shows.
const NOTICE_WIDTH: usize = 52;
const NOTICE_LINES: usize = 6;

/// What the stopped notice shows: the reason, the bug report's outcome and its code once it has
/// one (shown large), and the line of hints under it all (empty while the report is on its way).
pub struct NoticeShown<'a> {
    pub title: &'static str,
    pub text: &'a str,
    pub outcome: Option<String>,
    pub code: Option<&'a str>,
    pub hint: &'static str,
}

const PICKER_HINT: &str = "D-pad  choose   \u{b7}   A  go   \u{b7}   B / Select  back";
/// What the touch screen's hint line says in the menu.
/// Grid covers kept as textures. They are small, so in most libraries that is all of them.
pub const THUMBS: usize = 600;
/// Lines of text kept as textures before they are all let go.
const TEXTS: usize = 160;
/// The status bar's battery icon's height, in screen units.
const BATTERY_ICON: f32 = 22.0;
/// The status bar's Wi-Fi and Bluetooth icons' height, in screen units.
const RADIO_ICON: f32 = 22.0;

/// What the radio icons show: the radios, and how far a connecting Wi-Fi's fan has lit.
#[derive(Clone, Copy, PartialEq, Eq)]
struct RadioIcons {
    radios: nibkit::net::Radios,
    phase: u8,
}

/// The radios, as nibkit's monitor last read them: started the first time they are drawn,
/// read every few seconds on a thread of its own.
fn radios() -> &'static nibkit::net::Monitor {
    static MONITOR: OnceLock<nibkit::net::Monitor> = OnceLock::new();
    MONITOR.get_or_init(|| nibkit::net::Monitor::start(std::time::Duration::from_secs(3)))
}

/// When the connecting animation's clock started.
static START: OnceLock<Instant> = OnceLock::new();

/// A game's full-size artwork: its cart for the shelf, its box for the top screen.
struct Art {
    cart: Texture,
    cover: Texture,
    /// Its box again for the bottom screen's system picker, where the screens are two windows
    /// (a texture only draws on the canvas it was made for); `None` where they are one.
    blade: Option<Texture>,
}

impl Art {
    fn destroy(self) {
        destroy(self.cart);
        destroy(self.cover);
        if let Some(t) = self.blade {
            destroy(t);
        }
    }
}

/// A line of text, by what it says, its size in pixels, its weight and its canvas.
type TextKey = (String, u32, bool, usize);

pub struct Ui<'a> {
    /// One window spanning both screens, or one window for each. A texture only draws on the
    /// canvas it was made for.
    pub canvases: Vec<WindowCanvas>,
    pub(super) creators: Vec<TextureCreator<WindowContext>>,
    pub(super) fonts: &'a Fonts,
    /// The look (Game case, SNES cart shape) the artwork was made for.
    pub looks: art::Looks,
    loader: Loader,
    pub top: View,
    pub bot: View,
    arts: HashMap<usize, Art>,
    /// Grid covers, by game, one for each canvas.
    thumbs: HashMap<usize, Vec<Texture>>,
    /// Lines of text, with their width and height.
    texts: HashMap<TextKey, (Texture, u32, u32)>,
    /// A white pixel for each canvas, stretched and tinted for panels and highlights.
    pub(super) white: Vec<Texture>,
    /// The status bar with the DS slot, and with the slot-2 opening, by `Slot as usize`.
    /// The status bar with the slot in it: the DS's, then slot-2's opening sized for each
    /// console's carts (`bar_for`).
    bars: [Texture; 3],
    star: Vec<Texture>,
    heart: Vec<Texture>,
    /// The DSlot logo, for the screensaver, one for each canvas.
    logo: Vec<Texture>,
    /// The status bar's battery icon, as last drawn.
    battery: Option<(stats::BatteryNow, Texture)>,
    /// The system cards' photos, by console, made into textures as they are first shown.
    photos: HashMap<Console, Texture>,
    /// The status bar's Wi-Fi and Bluetooth icons, as last drawn, and what they showed.
    radios: Option<(RadioIcons, Texture, Texture)>,
    /// Games' resume points, read on a thread of their own as each game is shown.
    resumes: Resumes,
    /// The games' achievements (achievements/), looked up by a worker and drawn on the top
    /// screen (cheevos.rs).
    pub(super) cheevos: Cheevos,
    /// Where the status bar's Wi-Fi and Bluetooth icons were drawn (`true` Bluetooth's), for
    /// a tap on one.
    radio_boxes: Vec<(bool, (f32, f32, f32, f32))>,
    /// The menu rows' small pictures, by picture and height in pixels.
    glyphs: HashMap<(nibkit::menu::Glyph, usize), Texture>,
}

/// Resume points' pictures (resume.rs), read by a worker of their own and kept by game.
struct Resumes {
    ask: Sender<(usize, Wanted)>,
    told: Receiver<(usize, Wanted, Option<Option<Resume>>)>,
    /// What was found for each game, with what was asked: its picture on the top screen and
    /// when it was saved, or none.
    found: HashMap<usize, (Wanted, Option<(Texture, i64)>)>,
    asked: HashMap<usize, Wanted>,
}

impl Resumes {
    /// The worker: it reads the latest asked for, and tells the ones overtaken that they
    /// weren't read (`None`), so browsing quickly never queues reads up.
    fn start<'scope>(scope: &'scope Scope<'scope, '_>) -> Resumes {
        let (ask, asked) = channel::<(usize, Wanted)>();
        let (tell, told) = channel();
        scope.spawn(move || {
            while let Ok(first) = asked.recv() {
                let mut all: Vec<_> = std::iter::once(first).chain(asked.try_iter()).collect();
                let (g, wanted) = all.pop().expect("one at least");
                for (g, wanted) in all {
                    if tell.send((g, wanted, None)).is_err() {
                        return;
                    }
                }
                let found = crate::resume::find(&wanted);
                if tell.send((g, wanted, Some(found))).is_err() {
                    return;
                }
            }
        });
        Resumes {
            ask,
            told,
            found: HashMap::new(),
            asked: HashMap::new(),
        }
    }

    /// Forgets what was found, to be read again (a resume point cleared from the menu).
    fn forget(&mut self) {
        for (_, (_, t)) in self.found.drain() {
            if let Some((t, _)) = t {
                destroy(t);
            }
        }
    }
}

impl<'a> Ui<'a> {
    /// The textures that do not change, and the artwork workers, which stop with `scope`.
    pub fn new<'scope>(
        scope: &'scope Scope<'scope, 'a>,
        canvases: Vec<WindowCanvas>,
        (top, bot): (View, View),
        source: (&'a Fonts, &'a Shell, &'a PathBuf),
        looks: art::Looks,
        ra: &Ra,
    ) -> Result<Ui<'a>, String> {
        let (fonts, shell, cache) = source;
        let creators: Vec<_> = canvases.iter().map(|c| c.texture_creator()).collect();
        let each = |img: &Img| -> Result<Vec<Texture>, String> {
            creators.iter().map(|tc| texture(tc, img)).collect()
        };
        let white = each(&Img {
            w: 1,
            h: 1,
            px: vec![255; 4],
        })?;
        let bar = |slot, cart_w| texture(&creators[bot.c], &slot_bar(bot.s, slot, cart_w));
        let bars = [
            bar(Slot::One, 0.0)?,
            bar(Slot::Two, cart_geometry(Console::Gba).0)?,
            bar(Slot::Two, cart_geometry(Console::Gb).0)?,
        ];
        let icon = (ICON * bot.s).round() as usize;
        let (star, heart) = (each(&gfx::star(icon))?, each(&gfx::heart(icon))?);
        let logo = gfx::decode_png(crate::splash::LOGO).ok_or("the logo is not a PNG")?;
        let logo = each(&logo)?;
        let thumb = (
            (GRID_COVER.0 * bot.s).round() as usize,
            (GRID_COVER.1 * bot.s).round() as usize,
        );
        Ok(Ui {
            loader: Loader::start(
                scope,
                Source {
                    fonts,
                    shell,
                    thumb,
                    cache,
                },
            ),
            canvases,
            creators,
            fonts,
            looks,
            top,
            bot,
            arts: HashMap::new(),
            thumbs: HashMap::new(),
            texts: HashMap::new(),
            white,
            bars,
            star,
            heart,
            logo,
            battery: None,
            radios: None,
            photos: HashMap::new(),
            resumes: Resumes::start(scope),
            cheevos: Cheevos::start(ra),
            radio_boxes: Vec::new(),
            glyphs: HashMap::new(),
        })
    }

    /// Clears every canvas to black.
    pub fn clear(&mut self) {
        for canvas in self.canvases.iter_mut() {
            canvas.set_clip_rect(None);
            canvas.set_draw_color(Color::RGB(0, 0, 0));
            canvas.clear();
        }
    }

    pub fn present(&mut self) {
        for canvas in self.canvases.iter_mut() {
            canvas.present();
        }
    }

    /// The battery icon for `b` in the box `(x, y, w, h)`, made again only when it changes.
    fn battery_icon(
        &mut self,
        v: View,
        b: stats::BatteryNow,
        (x, y, w, h): (f32, f32, f32, f32),
    ) -> Result<(), String> {
        if self.battery.as_ref().is_none_or(|(was, _)| *was != b) {
            let px = (h * v.s).round() as usize;
            let icon = texture(&self.creators[v.c], &gfx::battery(px, b.percent, b.mark))?;
            if let Some((_, old)) = self.battery.replace((b, icon)) {
                destroy(old);
            }
        }
        if let Some((_, icon)) = &self.battery {
            self.canvases[v.c].copy(icon, None, v.rect(x, y, w, h))?;
        }
        Ok(())
    }

    /// The status bar's radios, from `x`: Wi-Fi (off, connecting, or its signal), then
    /// Bluetooth (off, on, or connected, with how many when more than one). Left out where the
    /// system says nothing of them.
    /// The radios' icons in the status bar, after the battery and its percentage (where the
    /// shelf's bar and the menu's both put them).
    fn radios_in_bar(&mut self, v: View) -> Result<(), String> {
        let mut x = 40.0;
        if let Some(b) = stats::battery() {
            let text = format!("{}%", b.percent);
            x += BATTERY_ICON * 2.1 + 10.0 + self.fonts.width(&text, 24.0, true) + 24.0;
        }
        self.radio_icons(v, x)
    }

    /// Wi-Fi's and Bluetooth's icons from `x`: none while a radio is off; dim while it is on
    /// and nothing is connected; bright when connected (Wi-Fi with its signal, Bluetooth with a
    /// dot, or how many devices).
    fn radio_icons(&mut self, v: View, x: f32) -> Result<(), String> {
        let now = radios().now();
        // Connecting: the fan lights up from the dot out, over and over.
        let phase = (START.get_or_init(Instant::now).elapsed().as_millis() / 350 % 4) as u8 + 1;
        let shown = RadioIcons {
            radios: now,
            phase: if now.wifi == Wifi::Connecting {
                phase
            } else {
                0
            },
        };
        if self.radios.as_ref().is_none_or(|(was, ..)| *was != shown) {
            let px = (RADIO_ICON * v.s).round() as usize;
            let (on, off, faint) = ([200, 204, 212, 255], [70, 76, 92, 255], [70, 76, 92, 255]);
            let wifi = match now.wifi {
                Wifi::Connected { bars } => gfx::wifi(px, bars, on, off, false),
                Wifi::Connecting => gfx::wifi(px, phase, [154, 163, 184, 255], off, false),
                Wifi::Off | Wifi::Unknown => gfx::wifi(px, 0, on, faint, true),
            };
            let bluetooth = match now.bluetooth {
                Bluetooth::Connected { .. } => gfx::bluetooth(px, [235, 238, 244, 255], false),
                Bluetooth::On => gfx::bluetooth(px, [154, 163, 184, 255], false),
                Bluetooth::Off | Bluetooth::Unknown => gfx::bluetooth(px, faint, true),
            };
            let (wifi, bluetooth) = (
                texture(&self.creators[v.c], &wifi)?,
                texture(&self.creators[v.c], &bluetooth)?,
            );
            if let Some((_, a, b)) = self.radios.replace((shown, wifi, bluetooth)) {
                destroy(a);
                destroy(b);
            }
        }
        let Some((_, wifi, bluetooth)) = &self.radios else {
            return Ok(());
        };
        let (y, h) = (SH - 59.0, RADIO_ICON);
        let mut x = x;
        self.radio_boxes.clear();
        if !matches!(now.wifi, Wifi::Unknown | Wifi::Off) {
            self.canvases[v.c].copy(wifi, None, v.rect(x, y, h * 1.3, h))?;
            self.radio_boxes.push((false, (x, y, h * 1.3, h)));
            x += h * 1.3 + 16.0;
        }
        if !matches!(now.bluetooth, Bluetooth::Unknown | Bluetooth::Off) {
            self.canvases[v.c].copy(bluetooth, None, v.rect(x, y, h * 0.72, h))?;
            self.radio_boxes.push((true, (x, y, h * 0.72, h)));
            if let Bluetooth::Connected { devices, .. } = now.bluetooth {
                if devices == 1 {
                    // Connected: a dot beside the rune.
                    self.white[v.c].set_color_mod(235, 238, 244);
                    self.white[v.c].set_alpha_mod(255);
                    let d = h * 0.24;
                    let at = v.rect(x + h * 0.72 + 4.0, y + h - d, d, d);
                    self.canvases[v.c].copy(&self.white[v.c], None, at)?;
                }
                if devices > 1 {
                    let count = devices.to_string();
                    self.text(
                        v,
                        &count,
                        (18.0, true),
                        (x + h * 0.72 + 3.0, y + 4.0),
                        0.0,
                        DIM,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// What is drawn on each screen now, top and bottom, where each has a canvas of its own
    /// (nibOS's panels): read back before it is presented.
    pub fn read_screens(&self) -> Option<[Img; 2]> {
        if self.top.c == self.bot.c {
            return None;
        }
        let read = |c: usize| -> Option<Img> {
            let (w, h) = self.canvases[c].output_size().ok()?;
            let px = self.canvases[c]
                .read_pixels(None, PixelFormatEnum::ABGR8888)
                .ok()?;
            Some(Img {
                w: w as usize,
                h: h as usize,
                px,
            })
        };
        Some([read(self.top.c)?, read(self.bot.c)?])
    }

    /// Both screens as RGBA pixels, with their size: the one canvas spanning them (a preview),
    /// or, with a canvas each (nibOS's panels), the top one over the bottom one.
    pub fn read_pixels(&self) -> Result<(u32, u32, Vec<u8>), String> {
        if let Some([top, bot]) = self.read_screens() {
            let w = top.w.max(bot.w);
            let mut px = Vec::with_capacity(w * (top.h + bot.h) * 4);
            for img in [&top, &bot] {
                for row in img.px.chunks(img.w * 4) {
                    px.extend_from_slice(row);
                    px.resize(px.len() + (w - img.w) * 4, 0);
                }
            }
            return Ok((w as u32, (top.h + bot.h) as u32, px));
        }
        let (w, h) = self.canvases[0].output_size()?;
        Ok((
            w,
            h,
            self.canvases[0].read_pixels(None, PixelFormatEnum::ABGR8888)?,
        ))
    }
}

impl Ui<'_> {
    /// Forget all artwork, to be composed again as it is wanted.
    pub fn forget_art(&mut self) {
        self.loader.forget();
        for (_, a) in self.arts.drain() {
            a.destroy();
        }
        for t in self.thumbs.drain().flat_map(|(_, ts)| ts) {
            destroy(t);
        }
    }

    /// Take in artwork the workers have finished and hand out more. `wants` and `hold` are
    /// places in `list`: what to make, most wanted first, and whose full-size artwork to keep.
    /// Returns whether everything wanted is here.
    pub fn load(
        &mut self,
        games: &[Game],
        list: &[usize],
        wants: impl Iterator<Item = Job>,
        hold: &HashSet<usize>,
        sel: usize,
    ) -> Result<bool, String> {
        for (job, made) in self.loader.take() {
            match (job, made) {
                (Job::Art(g), Made::Art { cart, cover }) => {
                    let blade = if self.top.c == self.bot.c {
                        None
                    } else {
                        Some(texture(&self.creators[self.bot.c], &cover)?)
                    };
                    let (cart, cover) = (
                        texture(&self.creators[self.bot.c], &cart)?,
                        texture(&self.creators[self.top.c], &cover)?,
                    );
                    self.arts.insert(g, Art { cart, cover, blade });
                }
                (Job::Thumb(g), Made::Thumb(img)) => {
                    // Shown in the grid, and on the top screen while the full-size box is made.
                    let each: Result<Vec<_>, _> =
                        self.creators.iter().map(|tc| texture(tc, &img)).collect();
                    self.thumbs.insert(g, each?);
                }
                _ => {}
            }
        }

        let held: HashSet<usize> = hold.iter().filter_map(|&i| list.get(i).copied()).collect();
        let gone: Vec<usize> = self
            .arts
            .keys()
            .copied()
            .filter(|g| !held.contains(g))
            .collect();
        for g in gone {
            if let Some(a) = self.arts.remove(&g) {
                a.destroy();
            }
        }
        if self.thumbs.len() > THUMBS {
            // A very large library: keep the covers around the selection.
            let around: HashSet<usize> = list
                .iter()
                .skip(sel.saturating_sub(THUMBS / 2))
                .take(THUMBS)
                .copied()
                .collect();
            let gone: Vec<usize> = self
                .thumbs
                .keys()
                .copied()
                .filter(|g| !around.contains(g))
                .collect();
            for g in gone {
                self.thumbs
                    .remove(&g)
                    .into_iter()
                    .flatten()
                    .for_each(destroy);
            }
        }

        let mut settled = self.loader.idle();
        for want in wants {
            // Places past either end of the list wrap round to nothing.
            let (place, have) = match want {
                Job::Art(i) => (
                    i,
                    list.get(i)
                        .map(|&g| (Job::Art(g), self.arts.contains_key(&g))),
                ),
                Job::Thumb(i) => (
                    i,
                    list.get(i)
                        .map(|&g| (Job::Thumb(g), self.thumbs.contains_key(&g))),
                ),
            };
            let Some((job, false)) = have else { continue };
            settled = false;
            if !self.loader.free() {
                break;
            }
            if !self.loader.asked(job) {
                let game = &games[list[place]];
                self.loader.ask(job, game, self.looks);
            }
        }
        Ok(settled)
    }

    /// Draws one line of text, `size` units high, its top at `y`. `anchor` is where `x` is
    /// along it: 0 at its left end, 0.5 in its middle, 1 at its right end.
    pub fn text(
        &mut self,
        v: View,
        s: &str,
        (size, bold): (f32, bool),
        (x, y): (f32, f32),
        anchor: f32,
        c: Color,
    ) -> Result<(), String> {
        if s.is_empty() {
            return Ok(());
        }
        let px = (size * v.s).round() as u32;
        let key = (s.to_owned(), px, bold, v.c);
        if !self.texts.contains_key(&key) {
            if self.texts.len() > TEXTS {
                self.texts.drain().for_each(|(_, (t, _, _))| destroy(t));
            }
            let img = self.fonts.render(s, px as f32, bold);
            self.texts.insert(
                key.clone(),
                (
                    texture(&self.creators[v.c], &img)?,
                    img.w as u32,
                    img.h as u32,
                ),
            );
        }
        let Some((t, w, h)) = self.texts.get_mut(&key) else {
            return Ok(());
        };
        t.set_color_mod(c.r, c.g, c.b);
        let dst = Rect::new(
            (v.ox + x * v.s - anchor * *w as f32).round() as i32,
            (v.oy + y * v.s).round() as i32,
            *w,
            *h,
        );
        self.canvases[v.c].copy(t, None, dst)
    }

    /// The screensaver, on the screens in `screens` (top, bottom): black, and the logo where
    /// the bouncer has it, in its colour (the screens stacked with the hinge between them).
    /// Only those canvases are cleared and shown again; a screen left black stays as it is.
    pub fn draw_saver(&mut self, b: &Bouncer, screens: [bool; 2]) -> Result<(), String> {
        let views = [(self.top, 0.0), (self.bot, SH + bouncer::HINGE)];
        // A canvas shared by both screens (the stock OS) is drawn whole.
        let mut canvases: Vec<usize> = (0..2)
            .filter(|&i| screens[i])
            .map(|i| views[i].0.c)
            .collect();
        canvases.dedup();
        for &c in &canvases {
            self.canvases[c].set_clip_rect(None);
            self.canvases[c].set_draw_color(Color::RGB(0, 0, 0));
            self.canvases[c].clear();
        }
        let (r, g, bl) = bouncer::colour(b.hue);
        for (v, top) in views {
            let y = b.y - top;
            if !canvases.contains(&v.c) || !bouncer::on_screen(y) {
                continue;
            }
            let logo = &mut self.logo[v.c];
            logo.set_color_mod(r, g, bl);
            self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
            // Where it is to a fraction of a pixel, so the slow drift glides between frames.
            let dst = v.frect(b.x, y, bouncer::LOGO.0, bouncer::LOGO.1);
            self.canvases[v.c].copy_f(logo, None, dst)?;
            self.canvases[v.c].set_clip_rect(None);
        }
        for c in canvases {
            self.canvases[c].present();
        }
        Ok(())
    }

    /// Darkens the whole screen by `alpha`.
    pub fn shade(&mut self, v: View, alpha: f32) -> Result<(), String> {
        if alpha <= 0.0 {
            return Ok(());
        }
        self.white[v.c].set_color_mod(0, 0, 0);
        self.white[v.c].set_alpha_mod((alpha * 255.0) as u8);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(0.0, 0.0, SW, SH))
    }

    /// A rating: five stars, the first `rating` of them lit.
    pub fn stars(&mut self, v: View, x: f32, y: f32, rating: u8) -> Result<(), String> {
        for i in 0..5 {
            let c = if i < rating {
                GOLD
            } else {
                Color::RGB(52, 58, 74)
            };
            self.star[v.c].set_color_mod(c.r, c.g, c.b);
            self.canvases[v.c].copy(
                &self.star[v.c],
                None,
                v.rect(x + i as f32 * (ICON + 4.0), y, ICON, ICON),
            )?;
        }
        Ok(())
    }

    pub fn heart(&mut self, v: View, x: f32, y: f32, c: Color) -> Result<(), String> {
        self.heart[v.c].set_color_mod(c.r, c.g, c.b);
        self.canvases[v.c].copy(&self.heart[v.c], None, v.rect(x, y, ICON, ICON))
    }

    /// Hint line, status bar and the fade to black that goes with a launch.
    pub fn footer(
        &mut self,
        v: View,
        hint: &str,
        fade: f32,
        console: Console,
    ) -> Result<(), String> {
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let hint = match hint {
            HINT if !art::has_shell(console) => START_HINT,
            hint => hint,
        };
        self.text(v, hint, (18.0, false), (SW / 2.0, 636.0), 0.5, FAINT)?;
        self.canvases[v.c].copy(
            &self.bars[bar_for(console)],
            None,
            v.rect(0.0, SH - BAR_H, SW, BAR_H),
        )?;
        // The battery, then the radios after it.
        if let Some(b) = stats::battery() {
            let (h, w) = (BATTERY_ICON, BATTERY_ICON * 2.1);
            self.battery_icon(v, b, (40.0, SH - 59.0, w, h))?;
            let text = format!("{}%", b.percent);
            self.text(
                v,
                &text,
                (24.0, true),
                (40.0 + w + 10.0, SH - 62.0),
                0.0,
                DIM,
            )?;
        }
        self.radios_in_bar(v)?;
        self.text(
            v,
            &stats::clock(),
            (24.0, true),
            (SW - 40.0, SH - 62.0),
            1.0,
            DIM,
        )?;
        self.shade(v, fade)
    }

    /// The top screen: the game's box, its name and details, its play time and rating.
    pub fn draw_top(
        &mut self,
        games: &[Game],
        g: usize,
        stats: &Stats,
        resume: Option<&Wanted>,
        fade: f32,
        cheevos_list: Option<ListView>,
    ) -> Result<(), String> {
        let v = self.top;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let game = &games[g];
        let resume = self.resume(g, resume)?;

        // Until the full-size box is ready, its grid cover stands in.
        let size = art::box_size(game.console);
        let cover = self
            .arts
            .get_mut(&g)
            .map(|a| &mut a.cover)
            .or(self.thumbs.get_mut(&g).map(|ts| &mut ts[v.c]));
        if let Some(cover) = cover {
            let (w, h) = (size.0 as f32, size.1 as f32);
            let dst = v.rect(300.0 - w / 2.0, SH / 2.0 - h / 2.0, w, h);
            let mut shadow = dst;
            shadow.offset((10.0 * v.s) as i32, (14.0 * v.s) as i32);
            // The shadow is the box itself in black, so it has the case's rounded corners.
            cover.set_color_mod(0, 0, 0);
            cover.set_alpha_mod(120);
            self.canvases[v.c].copy_ex(cover, None, shadow, -3.0, None, false, false)?;
            cover.set_color_mod(255, 255, 255);
            cover.set_alpha_mod(255);
            self.canvases[v.c].copy_ex(cover, None, dst, -3.0, None, false, false)?;
        }
        if let Some(saved) = resume {
            self.draw_resume(g, size, saved)?;
        }

        let (x, width) = (580.0, 400.0);
        let size = self.fonts.fit(game.name(), 58.0, 28.0, width, true);
        let mut y = 228.0;
        if stats.fave(&game.path) {
            self.heart(v, x, y - 44.0, RED)?;
        }
        self.text(v, game.name(), (size, true), (x, y), 0.0, WHITE)?;
        y += size * 1.3;
        // The title's other lines (a DS game's subtitle and publisher, an app's description),
        // each wrapped to the width, two lines in all.
        let mut lines = Vec::new();
        for line in game.title.iter().skip(1) {
            let room = 2 - lines.len();
            if room == 0 {
                break;
            }
            lines.extend(self.fonts.wrap(line, 26.0, false, width, room));
        }
        for line in lines {
            self.text(v, &line, (26.0, false), (x, y), 0.0, DIM)?;
            y += 34.0;
        }
        self.text(v, &game.serial(), (22.0, false), (x, y + 4.0), 0.0, FAINT)?;

        let rows = [
            ("Played", stats.played(&game.path)),
            ("Last played", stats.last_played(&game.path)),
            ("Sessions", stats.sessions(&game.path)),
        ];
        let y_list = y.max(380.0) + 20.0;
        let mut y = y.max(380.0) + 70.0;
        // Browsing the achievements: their list takes the details' place.
        if cheevos_list.is_none() {
            for (label, value) in rows {
                self.text(v, label, (22.0, false), (x, y), 0.0, DIM)?;
                self.text(v, &value, (22.0, true), (x + 150.0, y), 0.0, WHITE)?;
                y += 36.0;
            }
            self.text(v, "Rating", (22.0, false), (x, y), 0.0, DIM)?;
            self.stars(v, x + 150.0, y + 2.0, stats.rating(&game.path))?;
        }
        let game = game.clone();
        self.draw_cheevos(&game, cheevos_list, (x, y + 52.0), y_list)?;
        self.shade(v, fade)
    }

    /// The resume point `wanted` says to show for game `g`, when it was saved, if it has one
    /// whose picture is in: asked for the first time `g` is shown (or after it changed).
    fn resume(&mut self, g: usize, wanted: Option<&Wanted>) -> Result<Option<i64>, String> {
        let rs = &mut self.resumes;
        for (game, asked, found) in rs.told.try_iter().collect::<Vec<_>>() {
            if rs.asked.get(&game) == Some(&asked) {
                rs.asked.remove(&game);
            }
            // Overtaken while browsing: asked again when the game is shown again.
            let Some(found) = found else { continue };
            let made = match found {
                Some(r) => Some((texture(&self.creators[self.top.c], &r.picture)?, r.saved)),
                None => None,
            };
            if let Some((_, Some((old, _)))) = rs.found.insert(game, (asked, made)) {
                destroy(old);
            }
        }
        let Some(wanted) = wanted else {
            return Ok(None);
        };
        match rs.found.get(&g) {
            Some((asked, made)) if asked == wanted => Ok(made.as_ref().map(|(_, at)| *at)),
            _ => {
                if rs.asked.get(&g) != Some(wanted) && rs.ask.send((g, wanted.clone())).is_ok() {
                    rs.asked.insert(g, wanted.clone());
                }
                Ok(None)
            }
        }
    }

    /// Which radio's icon in the status bar a tap at `(x, y)` is on, with some room round it:
    /// `Some(true)` Bluetooth's, `Some(false)` Wi-Fi's.
    pub fn radio_hit(&self, x: f32, y: f32) -> Option<bool> {
        self.radio_boxes
            .iter()
            .find(|(_, (bx, by, w, h))| {
                (bx - 14.0..bx + w + 14.0).contains(&x) && (by - 16.0..by + h + 16.0).contains(&y)
            })
            .map(|&(bluetooth, _)| bluetooth)
    }

    /// Whether artwork, a resume point or achievements are still to come in: they want
    /// drawing when they do.
    pub fn waiting(&self) -> bool {
        !self.loader.idle() || !self.resumes.asked.is_empty() || self.cheevos.waiting()
    }

    /// What the status bar shows that changes by itself: the clock, the battery and the
    /// radios. `None` while it is animating (Wi-Fi connecting).
    pub fn status(&self) -> Option<(String, Option<stats::BatteryNow>, nibkit::net::Radios)> {
        let radios = radios().now();
        (radios.wifi != Wifi::Connecting).then(|| (stats::clock(), stats::battery(), radios))
    }

    /// Forgets the resume points read, to be read again.
    pub fn forget_resumes(&mut self) {
        self.resumes.forget();
    }

    /// Game `g`'s resume picture, inset over the bottom right corner of its box (`size`, drawn
    /// about (300, the screen's middle)): a third of the box's width, in a thin frame with a
    /// shadow, a "Resume" tag, and when it was saved under it. A DS game's shows both screens,
    /// one over the other.
    fn draw_resume(&mut self, g: usize, size: (usize, usize), saved: i64) -> Result<(), String> {
        let v = self.top;
        let Some((_, Some((picture, _)))) = self.resumes.found.get(&g) else {
            return Ok(());
        };
        let q = picture.query();
        let (bw, bh) = (size.0 as f32, size.1 as f32);
        let w = (bw * 0.36).clamp(120.0, 190.0);
        let h = w * q.height as f32 / q.width.max(1) as f32;
        // Over the corner, but clear of the details beside the box (from x = 580).
        let x = (300.0 + bw / 2.0 - w * 0.78).min(560.0 - w);
        let y = SH / 2.0 + bh / 2.0 - h * 0.7;
        let fill = |ui: &mut Self, c: Color, a: u8, (x, y, w, h): (f32, f32, f32, f32)| {
            ui.white[v.c].set_color_mod(c.r, c.g, c.b);
            ui.white[v.c].set_alpha_mod(a);
            ui.canvases[v.c].copy(&ui.white[v.c], None, v.rect(x, y, w, h))
        };
        let black = Color::RGB(0, 0, 0);
        fill(self, black, 90, (x + 4.0, y + 8.0, w + 6.0, h + 6.0))?;
        fill(self, black, 70, (x + 8.0, y + 12.0, w + 6.0, h + 6.0))?;
        fill(
            self,
            Color::RGB(236, 238, 244),
            255,
            (x - 3.0, y - 3.0, w + 6.0, h + 6.0),
        )?;
        if let Some((_, Some((picture, _)))) = self.resumes.found.get(&g) {
            self.canvases[v.c].copy(picture, None, v.rect(x, y, w, h))?;
        }
        // A DS game's two screens (taller, at the same width): a hinge between them.
        if (q.width, q.height) == (256, 384) {
            fill(
                self,
                Color::RGB(40, 44, 56),
                255,
                (x, y + h / 2.0 - 2.0, w, 4.0),
            )?;
        }
        // The tag, over the picture's top left corner.
        let tag = "Resume";
        let tw = self.fonts.width(tag, 16.0, true) + 16.0;
        fill(self, GOLD, 255, (x - 3.0, y - 15.0, tw, 24.0))?;
        self.text(
            v,
            tag,
            (16.0, true),
            (x + 5.0, y - 13.0),
            0.0,
            Color::RGB(30, 24, 8),
        )?;
        let when = crate::stats::saved_at(saved);
        self.text(
            v,
            &when,
            (16.0, false),
            (x + w + 3.0, y + h + 8.0),
            1.0,
            DIM,
        )
    }

    /// The carts over the slot. `carousel` is where the row of carts is (a place in the list,
    /// between two while it moves), `pull` how far the selected one has moved toward the slot.
    pub fn draw_carts(&mut self, page: &Page, carousel: f32, pull: f32) -> Result<(), String> {
        let Page {
            games: _,
            list,
            sel,
            console,
            heading,
            strip,
            fade,
        } = *page;
        let (cart_w, cart_h, pitch) = cart_geometry(console);
        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.heading(heading, &strip)?;

        // Furthest carts first, so the centre one sits on top.
        let p = carousel;
        let shown = |i: &usize| (*i as f32 - p).abs() < 2.6 && self.arts.contains_key(&list[*i]);
        let mut order: Vec<usize> = (0..list.len()).filter(shown).collect();
        order.sort_by(|a, b| (*b as f32 - p).abs().total_cmp(&(*a as f32 - p).abs()));
        for i in order {
            let d = i as f32 - p;
            let away = d.abs().min(1.0);
            let w = cart_w * (1.0 - 0.3 * away);
            let h = w * cart_h / cart_w;
            let y = CART_Y + if i == sel { pull } else { 0.0 };
            let dst = v.rect(SW / 2.0 + d * pitch - w / 2.0, y - h / 2.0, w, h);
            if let Some(a) = self.arts.get_mut(&list[i]) {
                a.cart.set_alpha_mod((255.0 - 150.0 * away) as u8);
                self.canvases[v.c].copy(&a.cart, None, dst)?;
            }
        }

        self.footer(v, HINT, fade, console)
    }

    /// A wall of boxes in place of the carts, `scroll` rows down.
    pub fn draw_grid(&mut self, page: &Page, scroll: f32, stats: &Stats) -> Result<(), String> {
        let Page {
            games,
            list,
            sel,
            console,
            heading,
            strip,
            fade,
        } = *page;
        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.heading(heading, &strip)?;
        if list.is_empty() {
            return self.footer(v, HINT, fade, console);
        }

        self.canvases[v.c].set_clip_rect(v.rect(
            0.0,
            GRID_AT.1 - 8.0,
            SW,
            GRID_ROWS as f32 * GRID_STEP.1 - 8.0,
        ));
        let first = scroll.max(0.0) as usize * COLS;
        for (place, &g) in list
            .iter()
            .enumerate()
            .skip(first)
            .take(COLS * (GRID_ROWS + 1))
        {
            let (x, y) = grid_cell(place, scroll);
            let cell = (GRID_COVER.0 as usize, GRID_COVER.1 as usize);
            let (w, h) = art::thumb_size(console, cell);
            let (w, h) = (w as f32, h as f32);
            let (x, y) = (x + (GRID_COVER.0 - w) / 2.0, y + (GRID_COVER.1 - h) / 2.0);
            self.white[v.c].set_alpha_mod(255);
            if place == sel {
                self.white[v.c].set_color_mod(WHITE.r, WHITE.g, WHITE.b);
                self.canvases[v.c].copy(
                    &self.white[v.c],
                    None,
                    v.rect(x - 5.0, y - 5.0, w + 10.0, h + 10.0),
                )?;
            }
            // The panel colour stands in for a cover that is not composed yet.
            self.white[v.c].set_color_mod(PANEL.r, PANEL.g, PANEL.b);
            self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, y, w, h))?;
            if let Some(t) = self.thumbs.get_mut(&g).map(|ts| &mut ts[v.c]) {
                t.set_color_mod(255, 255, 255);
                t.set_alpha_mod(if place == sel { 255 } else { 205 });
                self.canvases[v.c].copy(t, None, v.rect(x, y, w, h))?;
                t.set_alpha_mod(255);
            }
            if stats.fave(&games[g].path) {
                self.heart(v, x + w - ICON - 9.0, y + 11.0, Color::RGB(0, 0, 0))?;
                self.heart(v, x + w - ICON - 10.0, y + 9.0, RED)?;
            }
        }
        self.footer(v, HINT, fade, console)
    }

    /// The games as a list of names, `scroll` rows down; with `covers`, a small box beside
    /// each. The selected game's box is on the top screen as the list moves.
    pub fn draw_list(
        &mut self,
        page: &Page,
        scroll: f32,
        stats: &Stats,
        covers: bool,
    ) -> Result<(), String> {
        let Page {
            games,
            list,
            sel,
            console,
            heading,
            strip,
            fade,
        } = *page;
        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.heading(heading, &strip)?;
        if list.is_empty() {
            return self.footer(v, HINT, fade, console);
        }

        let (row_h, on_screen) = list_rows(covers);
        let (x0, w) = (LIST_AT.0, LIST_W);
        self.canvases[v.c].set_clip_rect(v.rect(0.0, LIST_AT.1, SW, on_screen as f32 * row_h));
        // A small box, as the grid's cover is shaped, made to fit the room beside the name.
        let (bw, bh) = {
            let cell = (GRID_COVER.0 as usize, GRID_COVER.1 as usize);
            let (w, h) = art::thumb_size(console, cell);
            let fit = (LIST_COVER.0 / w as f32).min(LIST_COVER.1 / h as f32);
            (w as f32 * fit, h as f32 * fit)
        };
        let first = scroll.max(0.0) as usize;
        for (place, &g) in list.iter().enumerate().skip(first).take(on_screen + 1) {
            let y = list_row(place, scroll, covers);
            let lit = place == sel;
            if lit {
                self.white[v.c].set_alpha_mod(255);
                self.white[v.c].set_color_mod(PANEL.r, PANEL.g, PANEL.b);
                self.canvases[v.c].copy(
                    &self.white[v.c],
                    None,
                    v.rect(x0, y + 2.0, w, row_h - 4.0),
                )?;
                self.white[v.c].set_color_mod(GOLD.r, GOLD.g, GOLD.b);
                self.canvases[v.c].copy(
                    &self.white[v.c],
                    None,
                    v.rect(x0, y + 2.0, 5.0, row_h - 4.0),
                )?;
            }
            let mut x = x0 + 22.0;
            if covers {
                let (bx, by) = (x + (LIST_COVER.0 - bw) / 2.0, y + (row_h - bh) / 2.0);
                self.white[v.c].set_alpha_mod(255);
                self.white[v.c].set_color_mod(PANEL.r, PANEL.g, PANEL.b);
                self.canvases[v.c].copy(&self.white[v.c], None, v.rect(bx, by, bw, bh))?;
                if let Some(t) = self.thumbs.get_mut(&g).map(|ts| &mut ts[v.c]) {
                    t.set_color_mod(255, 255, 255);
                    t.set_alpha_mod(if lit { 255 } else { 205 });
                    self.canvases[v.c].copy(t, None, v.rect(bx, by, bw, bh))?;
                    t.set_alpha_mod(255);
                }
                x += LIST_COVER.0 + 22.0;
            }
            let game = &games[g];
            let fave = stats.fave(&game.path);
            // Room for the name, short of the heart at the end of the row.
            let room = x0 + w - x - ICON - 40.0;
            let ink = if lit { WHITE } else { DIM };
            let name = self.fonts.wrap(game.name(), 26.0, lit, room, 1);
            let name = name.first().map_or("", String::as_str).to_owned();
            if covers {
                // The title's next line (a DS game's subtitle, an app's description) under it.
                let more = game.title.get(1).cloned().unwrap_or_else(|| game.serial());
                let more = self.fonts.wrap(&more, 20.0, false, room, 1);
                self.text(v, &name, (26.0, lit), (x, y + 16.0), 0.0, ink)?;
                if let Some(more) = more.first() {
                    self.text(v, more, (20.0, false), (x, y + 52.0), 0.0, FAINT)?;
                }
            } else {
                self.text(
                    v,
                    &name,
                    (26.0, lit),
                    (x, y + (row_h - 32.0) / 2.0),
                    0.0,
                    ink,
                )?;
            }
            if fave {
                self.heart(v, x0 + w - ICON - 18.0, y + (row_h - ICON) / 2.0, RED)?;
            }
        }
        // Where the list is, down its right edge, when it is longer than the screen.
        let n = list.len();
        if n > on_screen {
            let (top, track) = (LIST_AT.1 + 4.0, on_screen as f32 * row_h - 8.0);
            let bar = (track * on_screen as f32 / n as f32).max(24.0);
            let at = (scroll / (n - on_screen) as f32).clamp(0.0, 1.0) * (track - bar);
            self.white[v.c].set_alpha_mod(255);
            self.white[v.c].set_color_mod(FAINT.r, FAINT.g, FAINT.b);
            self.canvases[v.c].copy(
                &self.white[v.c],
                None,
                v.rect(SW - 12.0, top + at, 4.0, bar),
            )?;
        }
        self.footer(v, HINT, fade, console)
    }

    /// The shelf's page tabs, the page on show lit, in their strip: scrolled as it is, faded
    /// toward an edge that has more tabs past it, with a chevron there.
    pub fn tab(&mut self, page: Console, pages: &[Console], strip: &Strip) -> Result<(), String> {
        let v = self.bot;
        let (_, y, _, h) = TAB;
        self.canvases[v.c].set_clip_rect(v.rect(strip.x, y - 2.0, strip.width, h + 4.0));
        for (i, &p) in pages.iter().enumerate() {
            let (x, y, w, h) = strip.tab(i);
            if x + w < strip.x || x > strip.x + strip.width {
                continue;
            }
            let on = p == page;
            let c = if on { WHITE } else { PANEL };
            self.white[v.c].set_color_mod(c.r, c.g, c.b);
            self.white[v.c].set_alpha_mod(255);
            self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, y, w - 2.0, h))?;
            let ink = if on { Color::RGB(20, 23, 30) } else { DIM };
            self.text(
                v,
                p.label(),
                (22.0, true),
                (x + (w - 2.0) / 2.0, y + 7.0),
                0.5,
                ink,
            )?;
        }
        // The edges with more past them: the tabs fade into the background, a chevron on top.
        let (back, on) = strip.more();
        for (more, left) in [(back, true), (on, false)] {
            if !more {
                continue;
            }
            const STEPS: usize = 10;
            for k in 0..STEPS {
                let t = (k as f32 + 0.5) / STEPS as f32;
                let w = STRIP_FADE / STEPS as f32;
                let x = if left {
                    strip.x + k as f32 * w
                } else {
                    strip.x + strip.width - (k + 1) as f32 * w
                };
                self.white[v.c].set_color_mod(0, 0, 0);
                self.white[v.c].set_alpha_mod(((1.0 - t) * 245.0) as u8);
                self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, y, w + 0.5, h))?;
            }
            let (cx, anchor) = if left {
                (strip.x + 4.0, 0.0)
            } else {
                (strip.x + strip.width - 4.0, 1.0)
            };
            let chevron = if left { "\u{2039}" } else { "\u{203a}" };
            self.text(v, chevron, (30.0, true), (cx, y + 1.0), anchor, WHITE)?;
        }
        self.white[v.c].set_alpha_mod(255);
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        Ok(())
    }

    /// How wide the heading is, as drawn: the room the tab strip leaves it.
    pub fn heading_width(&self, heading: &str) -> f32 {
        self.fonts.width(heading, 20.0, false).min(HEADING_MOST)
    }

    /// The line at the top of the touch screen: right-aligned with the status bar's clock, in the
    /// room the tab strip leaves it, smaller (and then cut short) only when even that is short.
    fn heading(&mut self, heading: &str, strip: &Strip) -> Result<(), String> {
        let v = self.bot;
        let room = strip.heading_room();
        let size = self.fonts.fit(heading, 20.0, 16.0, room, false);
        let line = self
            .fonts
            .wrap(heading, size, false, room, 1)
            .into_iter()
            .next()
            .unwrap_or_default();
        self.text(
            v,
            &line,
            (size, false),
            (HEADING_RIGHT, 24.0 + (20.0 - size) / 2.0),
            1.0,
            FAINT,
        )
    }

    /// A system's card on the top screen, as a Top Trumps card: its name and maker on a band in
    /// its colour, its stats down the left, each with a bar against every system's, a picture
    /// box on the right and what is on the cards for it under that, then its history and a fun
    /// fact, and where the facts come from.
    pub fn draw_card(
        &mut self,
        console: Console,
        card: &Card,
        yours: &Yours,
    ) -> Result<(), String> {
        let v = self.top;
        let colour = console_colour(console);
        let light = |c: Color, k: f32| {
            Color::RGB(
                (c.r as f32 + (255.0 - c.r as f32) * k) as u8,
                (c.g as f32 + (255.0 - c.g as f32) * k) as u8,
                (c.b as f32 + (255.0 - c.b as f32) * k) as u8,
            )
        };
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let fill = |ui: &mut Self, c: Color, (x, y, w, h): (f32, f32, f32, f32)| {
            ui.white[v.c].set_color_mod(c.r, c.g, c.b);
            ui.white[v.c].set_alpha_mod(255);
            ui.canvases[v.c].copy(&ui.white[v.c], None, v.rect(x, y, w, h))
        };
        fill(self, Color::RGB(0, 0, 0), (0.0, 0.0, SW, SH))?;
        // The card: a frame in the system's colour, a dark face, a band across the top.
        let (x0, y0, w0, h0) = (20.0, 20.0, SW - 40.0, SH - 40.0);
        fill(self, colour, (x0, y0, w0, h0))?;
        fill(
            self,
            Color::RGB(22, 26, 36),
            (x0 + 6.0, y0 + 6.0, w0 - 12.0, h0 - 12.0),
        )?;
        fill(self, colour, (x0 + 6.0, y0 + 6.0, w0 - 12.0, 104.0))?;
        self.text(v, &card.name, (52.0, true), (52.0, 34.0), 0.0, WHITE)?;
        self.text(
            v,
            &card.maker,
            (22.0, false),
            (54.0, 92.0),
            0.0,
            light(colour, 0.7),
        )?;
        // The stats, each with its bar.
        let (left, right) = (52.0, 580.0);
        let mut y = 152.0;
        for stat in card.stats.iter().take(7) {
            self.text(v, &stat.label, (20.0, false), (left, y), 0.0, DIM)?;
            let size = self
                .fonts
                .fit(&stat.value, 22.0, 16.0, right - left - 150.0, true);
            self.text(
                v,
                &stat.value,
                (size, true),
                (right, y + (22.0 - size) / 2.0),
                1.0,
                WHITE,
            )?;
            fill(
                self,
                Color::RGB(44, 50, 64),
                (left, y + 30.0, right - left, 7.0),
            )?;
            let t = match (stat.share, stat.bar) {
                (Some(share), _) => Some(share.clamp(0.0, 1.0) as f32),
                (None, Some(value)) => Some(crate::systems::bar(&stat.label, value)),
                _ => None,
            };
            if let Some(t) = t {
                fill(
                    self,
                    light(colour, 0.45),
                    (left, y + 30.0, (right - left) * t, 7.0),
                )?;
            }
            y += 46.0;
        }
        // The picture: a photo of the system, framed in its colour; without one, its name.
        let pic = (620.0, 152.0, 352.0, 236.0);
        fill(self, colour, pic)?;
        let rim = (pic.0 + 3.0, pic.1 + 3.0, pic.2 - 6.0, pic.3 - 6.0);
        fill(self, Color::RGB(0, 0, 0), rim)?;
        let (ix, iy, iw, ih) = (pic.0 + 6.0, pic.1 + 6.0, pic.2 - 12.0, pic.3 - 12.0);
        if self.card_photo(console)? {
            if let Some(photo) = self.photos.get(&console) {
                self.canvases[v.c].copy(photo, None, v.rect(ix, iy, iw, ih))?;
            }
        } else {
            fill(self, dim_by(colour, 0.6), (ix, iy, iw, ih))?;
            let label = console.label();
            let size = self.fonts.fit(label, 96.0, 40.0, pic.2 - 40.0, true);
            let at = (pic.0 + pic.2 / 2.0, pic.1 + pic.3 / 2.0 - size * 0.62);
            self.text(v, label, (size, true), at, 0.5, light(colour, 0.35))?;
        }
        // What is on the cards for it.
        let (bx, by) = (620.0, 404.0);
        self.text(v, "On your cards", (18.0, false), (bx, by), 0.0, DIM)?;
        let games = match yours.games {
            1 => "1 game".to_owned(),
            n => format!("{n} games"),
        };
        let line = match yours.secs / 60 {
            0 => games,
            m if m < 60 => format!("{games}  \u{b7}  {m} min played"),
            m => format!("{games}  \u{b7}  {} h {:02} min played", m / 60, m % 60),
        };
        self.text(v, &line, (22.0, true), (bx, by + 26.0), 0.0, WHITE)?;
        if let Some(most) = &yours.most {
            let most =
                self.fonts
                    .wrap(&format!("{}: {most}", yours.most_is), 20.0, false, 352.0, 1);
            if let Some(most) = most.first() {
                self.text(v, most, (20.0, false), (bx, by + 58.0), 0.0, DIM)?;
            }
        }
        // Its history, the fun fact, and where the facts come from.
        let width = SW - 2.0 * left;
        let mut y = 492.0;
        for line in self.fonts.wrap(&card.bio, 21.0, false, width, 4) {
            self.text(
                v,
                &line,
                (21.0, false),
                (left, y),
                0.0,
                Color::RGB(206, 211, 222),
            )?;
            y += 28.0;
        }
        y += 8.0;
        let heading = "Fun fact";
        self.text(v, heading, (18.0, true), (left, y + 2.0), 0.0, GOLD)?;
        let indent = 96.0f32.max(self.fonts.width(heading, 18.0, true) + 18.0);
        for line in self.fonts.wrap(&card.fact, 20.0, false, width - indent, 2) {
            self.text(v, &line, (20.0, false), (left + indent, y), 0.0, WHITE)?;
            y += 27.0;
        }
        if !card.source.is_empty() {
            let source = card
                .source
                .trim_start_matches("https://")
                .replace("%26", "&")
                .replace("%C2%B2", "\u{b2}");
            let facts = format!("Facts: {source}");
            self.text(v, &facts, (14.0, false), (left, SH - 46.0), 0.0, FAINT)?;
        }
        if let Some(photo) = crate::systems::photo(console) {
            let credit = photo.credit;
            self.text(v, credit, (14.0, false), (SW - left, SH - 46.0), 1.0, FAINT)?;
        }
        Ok(())
    }

    /// The Apps page's card, about the device (device.rs), in a system card's frame: its name
    /// and system on the band, what it is made of down the left, a drawing of it with its apps
    /// under it on the right, and how it is now across the bottom, the shares with bars.
    pub fn draw_about(&mut self, about: &About, apps: &Yours) -> Result<(), String> {
        let v = self.top;
        let colour = console_colour(Console::Apps);
        let light = |c: Color, k: f32| {
            Color::RGB(
                (c.r as f32 + (255.0 - c.r as f32) * k) as u8,
                (c.g as f32 + (255.0 - c.g as f32) * k) as u8,
                (c.b as f32 + (255.0 - c.b as f32) * k) as u8,
            )
        };
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        let fill = |ui: &mut Self, c: Color, (x, y, w, h): (f32, f32, f32, f32)| {
            ui.white[v.c].set_color_mod(c.r, c.g, c.b);
            ui.white[v.c].set_alpha_mod(255);
            ui.canvases[v.c].copy(&ui.white[v.c], None, v.rect(x, y, w, h))
        };
        fill(self, Color::RGB(0, 0, 0), (0.0, 0.0, SW, SH))?;
        let (x0, y0, w0, h0) = (20.0, 20.0, SW - 40.0, SH - 40.0);
        fill(self, colour, (x0, y0, w0, h0))?;
        fill(
            self,
            Color::RGB(22, 26, 36),
            (x0 + 6.0, y0 + 6.0, w0 - 12.0, h0 - 12.0),
        )?;
        fill(self, colour, (x0 + 6.0, y0 + 6.0, w0 - 12.0, 104.0))?;
        let size = self.fonts.fit(&about.name, 52.0, 32.0, SW - 120.0, true);
        self.text(
            v,
            &about.name,
            (size, true),
            (52.0, 34.0 + (52.0 - size) * 0.6),
            0.0,
            WHITE,
        )?;
        self.text(
            v,
            &about.system,
            (22.0, false),
            (54.0, 92.0),
            0.0,
            light(colour, 0.7),
        )?;
        // What it is made of.
        let (left, right) = (52.0, 580.0);
        let rule = Color::RGB(44, 50, 64);
        let mut y = 156.0;
        for spec in about.specs.iter().take(7) {
            self.text(v, &spec.label, (20.0, false), (left, y), 0.0, DIM)?;
            let size = self
                .fonts
                .fit(&spec.value, 22.0, 16.0, right - left - 110.0, true);
            let at = (right, y + (22.0 - size) / 2.0);
            self.text(v, &spec.value, (size, true), at, 1.0, WHITE)?;
            fill(self, rule, (left, y + 32.0, right - left, 2.0))?;
            y += 40.0;
        }
        // Its drawing, and its apps.
        let pic = (620.0, 152.0, 352.0, 236.0);
        fill(self, colour, pic)?;
        let rim = (pic.0 + 3.0, pic.1 + 3.0, pic.2 - 6.0, pic.3 - 6.0);
        fill(self, Color::RGB(0, 0, 0), rim)?;
        let inner = (pic.0 + 6.0, pic.1 + 6.0, pic.2 - 12.0, pic.3 - 12.0);
        fill(self, dim_by(colour, 0.6), inner)?;
        self.draw_device(inner, light(colour, 0.8), dim_by(colour, 0.35))?;
        let (bx, by) = (620.0, 404.0);
        self.text(v, "Apps", (18.0, false), (bx, by), 0.0, DIM)?;
        let count = match apps.games {
            1 => "1 app".to_owned(),
            n => format!("{n} apps"),
        };
        self.text(v, &count, (22.0, true), (bx, by + 26.0), 0.0, WHITE)?;
        if let Some(last) = &apps.most {
            let line = self
                .fonts
                .wrap(&format!("{}: {last}", apps.most_is), 20.0, false, 352.0, 1);
            if let Some(line) = line.first() {
                self.text(v, line, (20.0, false), (bx, by + 58.0), 0.0, DIM)?;
            }
        }
        // How it is now: two columns.
        let top = 500.0;
        let col = (SW - 2.0 * left - 48.0) / 2.0;
        for (i, stat) in about.now.iter().take(6).enumerate() {
            let x = left + (i % 2) as f32 * (col + 48.0);
            let y = top + (i / 2) as f32 * 50.0;
            self.text(v, &stat.label, (20.0, false), (x, y), 0.0, DIM)?;
            let size = self.fonts.fit(&stat.value, 22.0, 16.0, col - 130.0, true);
            let at = (x + col, y + (22.0 - size) / 2.0);
            self.text(v, &stat.value, (size, true), at, 1.0, WHITE)?;
            fill(self, rule, (x, y + 30.0, col, 7.0))?;
            if let Some(share) = stat.share {
                let w = col * share.clamp(0.0, 1.0) as f32;
                fill(self, light(colour, 0.45), (x, y + 30.0, w, 7.0))?;
            }
        }
        // What is on the cards, and what has been played most.
        let y = top + 3.0 * 50.0 + 4.0;
        self.text(
            v,
            &about.library,
            (20.0, false),
            (left, y),
            0.0,
            Color::RGB(206, 211, 222),
        )?;
        if let Some(most) = &about.most {
            let heading = "Most played";
            self.text(v, heading, (18.0, true), (left, y + 34.0), 0.0, GOLD)?;
            let indent = self.fonts.width(heading, 18.0, true) + 18.0;
            let line = self
                .fonts
                .wrap(most, 20.0, false, SW - 2.0 * left - indent, 1);
            if let Some(line) = line.first() {
                self.text(
                    v,
                    line,
                    (20.0, false),
                    (left + indent, y + 32.0),
                    0.0,
                    WHITE,
                )?;
            }
        }
        if !about.source.is_empty() {
            let facts = format!("Facts: {}", about.source);
            self.text(v, &facts, (14.0, false), (left, SH - 46.0), 0.0, FAINT)?;
        }
        Ok(())
    }

    /// The device, drawn for the Apps card: an open clamshell, face on, in `body` and `ink`,
    /// fitted in `(x, y, w, h)` on the top screen. Drawn, not a photo: no maker's picture.
    fn draw_device(
        &mut self,
        (x, y, w, h): (f32, f32, f32, f32),
        body: Color,
        ink: Color,
    ) -> Result<(), String> {
        let v = self.top;
        // In units of a 100 x 140 device: two 100 x 66 halves either side of an 8-high hinge.
        let u = (h - 16.0) / 140.0;
        let (ox, oy) = (x + (w - 100.0 * u) / 2.0, y + 8.0);
        let put = |ui: &mut Self, c: Color, (rx, ry, rw, rh): (f32, f32, f32, f32)| {
            ui.white[v.c].set_color_mod(c.r, c.g, c.b);
            ui.white[v.c].set_alpha_mod(255);
            ui.canvases[v.c].copy(
                &ui.white[v.c],
                None,
                v.rect(ox + rx * u, oy + ry * u, rw * u, rh * u),
            )
        };
        let screen = Color::RGB(36, 44, 58);
        let glass = Color::RGB(70, 86, 110);
        // A half: its shell with the corners taken in, its screen in a bezel.
        let half = |ui: &mut Self, top: f32, sw: f32, sh: f32, sy: f32| -> Result<(), String> {
            put(ui, body, (2.0, top, 96.0, 66.0))?;
            put(ui, body, (0.0, top + 2.0, 100.0, 62.0))?;
            put(
                ui,
                ink,
                ((100.0 - sw) / 2.0 - 2.0, top + sy - 2.0, sw + 4.0, sh + 4.0),
            )?;
            put(ui, screen, ((100.0 - sw) / 2.0, top + sy, sw, sh))?;
            // A glint across the glass.
            put(
                ui,
                glass,
                ((100.0 - sw) / 2.0 + 3.0, top + sy + 3.0, sw * 0.4, 2.0),
            )
        };
        half(self, 0.0, 72.0, 54.0, 6.0)?;
        // The hinge.
        put(self, ink, (6.0, 66.0, 88.0, 8.0))?;
        put(self, body, (10.0, 67.5, 80.0, 5.0))?;
        half(self, 74.0, 56.0, 42.0, 6.0)?;
        // The d-pad, the four buttons, Start and Select.
        let b = 74.0;
        put(self, ink, (7.0, b + 22.0, 15.0, 5.0))?;
        put(self, ink, (12.0, b + 17.0, 5.0, 15.0))?;
        for (bx, by) in [
            (85.0, b + 17.0),
            (90.0, b + 22.0),
            (80.0, b + 22.0),
            (85.0, b + 27.0),
        ] {
            put(self, ink, (bx, by, 5.0, 5.0))?;
        }
        put(self, ink, (80.0, b + 44.0, 6.0, 3.0))?;
        put(self, ink, (88.0, b + 44.0, 6.0, 3.0))?;
        Ok(())
    }

    /// Whether `console`'s card has a photo, made into a texture (kept in `photos`) the first
    /// time it is wanted.
    fn card_photo(&mut self, console: Console) -> Result<bool, String> {
        let Some(png) = crate::systems::photo(console).map(|p| p.png) else {
            return Ok(false);
        };
        if !self.photos.contains_key(&console) {
            let img = gfx::decode_png(png).ok_or("a card's photo is not a PNG")?;
            let t = texture(&self.creators[self.top.c], &img)?;
            self.photos.insert(console, t);
        }
        Ok(true)
    }

    /// The system picker over the touch screen: a blade for each page, the stack at `pos` (the
    /// chosen blade's place, between two while it glides), `at` the chosen one. A blade is the
    /// system's colour, with box art from one of its games over it (cropped to fill it, kept
    /// crisp) where there is some; the chosen one is bright, with the system's name, its games
    /// and when one was last played, and that game's cart; the others are dimmed, more the
    /// further away. With `look.lean`, the edges between them slant, and all on a blade with
    /// them.
    pub fn draw_picker(
        &mut self,
        shown: &[Blade],
        look: Blades,
        pos: f32,
        at: usize,
    ) -> Result<(), String> {
        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.white[v.c].set_color_mod(0, 0, 0);
        self.white[v.c].set_alpha_mod(255);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(0.0, 0.0, SW, SH - BAR_H))?;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, BLADES_TOP, SW, BLADES_BOTTOM - BLADES_TOP));
        self.canvases[v.c].set_blend_mode(BlendMode::Blend);
        let lean = look.lean.to_radians().tan();
        // How far a slanted edge is above (or below) where it would be, at `x`.
        let slant = |x: f32| lean * (x - SW / 2.0);
        let px = |x: f32, y: f32| sdl2::rect::FPoint::new(v.ox + x * v.s, v.oy + y * v.s);
        let quad = |top: f32, bottom: f32, c: Color| -> Vec<sdl2::render::Vertex> {
            [(0.0, top), (SW, top), (SW, bottom), (0.0, bottom)]
                .iter()
                .map(|&(x, y)| sdl2::render::Vertex {
                    position: px(x, y + slant(x)),
                    color: c,
                    tex_coord: sdl2::rect::FPoint::new(0.0, 0.0),
                })
                .collect()
        };
        const TWO: [u8; 6] = [0, 1, 2, 0, 2, 3];
        let reach = lean.abs() * SW / 2.0;
        let blades: Vec<(usize, f32, f32, f32)> = (0..shown.len())
            .map(|i| {
                let (top, h) = look.blade(i, pos);
                let away = (i as f32 - pos).abs();
                let k = (1.0 - 0.38 * away.min(1.0) - 0.08 * (away - 1.0).max(0.0)).max(0.25);
                (i, top, h, k)
            })
            .filter(|&(_, top, h, _)| {
                h >= 6.0 && top + h > BLADES_TOP - reach && top < BLADES_BOTTOM + reach
            })
            .collect();
        let dim = |c: Color, k: f32| {
            Color::RGBA(
                (c.r as f32 * k) as u8,
                (c.g as f32 * k) as u8,
                (c.b as f32 * k) as u8,
                c.a,
            )
        };
        // First every blade's colour and art, then what is written on them, so that no blade
        // covers the words of the one before.
        for &(i, top, h, k) in &blades {
            let blade = &shown[i];
            let bottom = top + h - BLADE_GAP;
            let field = quad(top, bottom, dim(console_colour(blade.console), k));
            self.canvases[v.c].render_geometry(&field, None, &TWO[..])?;
            let art = blade.game.filter(|_| blade.art);
            let Some(cover) = art
                .and_then(|g| self.arts.get_mut(&g))
                .map(|a| a.blade.as_mut().unwrap_or(&mut a.cover))
            else {
                continue;
            };
            cover.set_scale_mode(sdl2::render::ScaleMode::Nearest);
            let q = cover.query();
            let (tw, th) = (q.width as f32, q.height as f32);
            // Cropped about its middle to the blade's shape (filling it, never stretched).
            let (y0, y1) = (top - reach, bottom + reach);
            let want = SW / (y1 - y0);
            let (cw, ch) = if tw / th > want {
                (th * want, th)
            } else {
                (tw, tw / want)
            };
            let (cx, cy) = ((tw - cw) / 2.0 / tw, (th - ch) / 2.0 / th);
            let mut art = quad(top, bottom, dim(Color::RGB(205, 205, 205), k));
            for vertex in &mut art {
                let (x, y) = (
                    (vertex.position.x - v.ox) / v.s,
                    (vertex.position.y - v.oy) / v.s,
                );
                vertex.tex_coord = sdl2::rect::FPoint::new(
                    cx + x / SW * cw / tw,
                    cy + (y - y0) / (y1 - y0) * ch / th,
                );
            }
            self.canvases[v.c].render_geometry(&art, Some(&*cover), &TWO[..])?;
        }
        for &(i, top, h, k) in &blades {
            let blade = &shown[i];
            let mid = top + h / 2.0;
            let name = blade.console.name();
            if i == at && h > 120.0 {
                // The chosen blade: a darker band behind its words, the cart at the right.
                let band = quad(mid - 58.0, mid + 58.0, Color::RGBA(0, 0, 0, 150));
                self.canvases[v.c].render_geometry(&band, None, &TWO[..])?;
                if let Some(cart) = blade.game.and_then(|g| self.arts.get(&g)).map(|a| &a.cart) {
                    let q = cart.query();
                    let ch = (h - 30.0).min(200.0);
                    let cw = ch * q.width as f32 / q.height as f32;
                    let x = SW - cw - 36.0;
                    let y = mid - ch / 2.0 + slant(x + cw / 2.0);
                    self.canvases[v.c].copy(cart, None, v.rect(x, y, cw, ch))?;
                }
                // The name in a frame of its own: outlined, a gold rule under it.
                let y = mid - 52.0 + slant(44.0);
                for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)] {
                    self.text(
                        v,
                        name,
                        (60.0, true),
                        (44.0 + dx, y + dy),
                        0.0,
                        Color::RGB(0, 0, 0),
                    )?;
                }
                self.text(v, name, (60.0, true), (44.0, y), 0.0, WHITE)?;
                self.white[v.c].set_color_mod(GOLD.r, GOLD.g, GOLD.b);
                self.white[v.c].set_alpha_mod(255);
                self.canvases[v.c].copy(
                    &self.white[v.c],
                    None,
                    v.rect(44.0, y + 70.0, 64.0, 3.0),
                )?;
                let (noun, last) = if blade.console == Console::Apps {
                    ("app", "Last used")
                } else {
                    ("game", "Last played")
                };
                let games = match blade.games {
                    1 => format!("1 {noun}"),
                    n => format!("{n} {noun}s"),
                };
                let line = match &blade.last {
                    Some(when) => format!("{games}   \u{b7}   {last} {when}"),
                    None => games,
                };
                self.text(v, &line, (22.0, false), (44.0, y + 80.0), 0.0, DIM)?;
            } else if h > 26.0 {
                let ink = dim(WHITE, k);
                let size = (h * 0.45).clamp(16.0, 30.0);
                let y = mid - size * 0.62 + slant(44.0 + size);
                self.text(v, name, (size, true), (44.0, y), 0.0, ink)?;
            }
        }
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.white[v.c].set_alpha_mod(255);
        self.footer(v, PICKER_HINT, 0.0, Console::Apps)
    }

    /// The menu on the touch screen, over the page's status bar.
    /// The menu, laid out by nibkit's `menu` (the same layout every app's menu uses: DSlot's,
    /// by construction) and drawn here with DSlot's own text, shapes, battery and slot bar.
    pub fn draw_menu(&mut self, menu: &Shown, console: Console) -> Result<(), String> {
        use nibkit::menu::{layout, Draw, Status};
        let laid_out = nibkit::menu::Menu {
            tabs: menu.tabs.clone(),
            title: menu.title.clone(),
            rows: menu.rows.iter().map(menu_row).collect(),
            row: menu.row,
            scroll: menu.scroll,
            hint: menu.hint.clone(),
        };
        let battery = stats::battery();
        let status = Status {
            clock: stats::clock(),
            battery: battery.map(|b| (b.percent, b.mark == gfx::Mark::Charging)),
            ..Status::default()
        };
        let fonts = self.fonts;
        let draws = match &menu.keyboard {
            // Typing: the keyboard, over the whole touch screen.
            Some(kb) => kb.draws_measured(|s, size, bold| fonts.width(s, size, bold)),
            None => layout::menu_measured(&laid_out, &status, |s, size, bold| {
                fonts.width(s, size, bold)
            }),
        };
        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        for draw in draws {
            match draw {
                Draw::Rect { x, y, w, h, colour }
                | Draw::Round {
                    x, y, w, h, colour, ..
                } => {
                    let [r, g, b] = colour;
                    self.white[v.c].set_color_mod(r, g, b);
                    self.white[v.c].set_alpha_mod(255);
                    self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, y, w, h))?;
                }
                Draw::Text {
                    text,
                    size,
                    bold,
                    x,
                    y,
                    anchor,
                    colour: [r, g, b],
                } => self.text(v, &text, (size, bold), (x, y), anchor, Color::RGB(r, g, b))?,
                Draw::Stars { x, y, rating } => self.stars(v, x, y, rating)?,
                // DSlot's own icon, which also shows the plug of a battery held on its charger.
                Draw::Battery { x, y, h, .. } => {
                    if let Some(b) = battery {
                        self.battery_icon(v, b, (x, y, h * 2.1, h))?;
                    }
                }
                Draw::Bar { y, .. } => {
                    self.canvases[v.c].copy(
                        &self.bars[bar_for(console)],
                        None,
                        v.rect(0.0, y, SW, BAR_H),
                    )?;
                }
                // An outline: its four sides (the keyboard's Done key, highlighted).
                Draw::Frame {
                    x,
                    y,
                    w,
                    h,
                    width,
                    colour: [r, g, b],
                    ..
                } => {
                    self.white[v.c].set_color_mod(r, g, b);
                    self.white[v.c].set_alpha_mod(255);
                    for (fx, fy, fw, fh) in [
                        (x, y, w, width),
                        (x, y + h - width, w, width),
                        (x, y, width, h),
                        (x + w - width, y, width, h),
                    ] {
                        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(fx, fy, fw, fh))?;
                    }
                }
                // A row's small picture: nibkit's shapes, made into a texture at this height.
                Draw::Glyph { glyph, x, y, h } => {
                    let key = self.glyph(glyph, h)?;
                    let tex = &self.glyphs[&key];
                    let w = tex.query().width as f32 / v.s;
                    self.canvases[v.c].copy(tex, None, v.rect(x, y, w, h))?;
                }
                // Not in a page of rows.
                Draw::Picture { .. } => {}
            }
        }
        // A QR code under the first row: dark modules on a white square with a quiet zone,
        // as big as the space down to the hint line allows.
        if let (Some(qr), None) = (&menu.picture, &menu.keyboard) {
            const QUIET: usize = 4;
            let top = layout::row_y(1) - layout::ROW_STEP / 2.0 + 8.0;
            let bottom = layout::HINT_Y - 20.0;
            let side = qr.size + 2 * QUIET;
            let px = ((bottom - top) / side as f32).floor().max(1.0);
            let x0 = ((SW - px * side as f32) / 2.0).floor();
            let y0 = top.floor();
            self.white[v.c].set_color_mod(255, 255, 255);
            self.white[v.c].set_alpha_mod(255);
            self.canvases[v.c].copy(
                &self.white[v.c],
                None,
                v.rect(x0, y0, px * side as f32, px * side as f32),
            )?;
            self.white[v.c].set_color_mod(16, 18, 24);
            for y in 0..qr.size {
                for x in 0..qr.size {
                    if qr.dark(x, y) {
                        let rx = x0 + (x + QUIET) as f32 * px;
                        let ry = y0 + (y + QUIET) as f32 * px;
                        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(rx, ry, px, px))?;
                    }
                }
            }
        }
        // The radios in the bar too, live as the Wi-Fi and Bluetooth pages change them (not
        // over the keyboard, which takes the whole screen).
        if menu.keyboard.is_none() {
            self.radios_in_bar(v)?;
        }
        Ok(())
    }

    /// A row's small picture (nibkit's `menu::glyphs`), `h` high on the touch screen, made the
    /// first time it is wanted and kept.
    fn glyph(
        &mut self,
        glyph: nibkit::menu::Glyph,
        h: f32,
    ) -> Result<(nibkit::menu::Glyph, usize), String> {
        let v = self.bot;
        let px = (h * v.s).round().max(1.0) as usize;
        if !self.glyphs.contains_key(&(glyph, px)) {
            let (on, off) = ([241, 243, 248], [70, 76, 92]);
            let (w, rgba) = nibkit::menu::glyphs::render(glyph, px, on, off);
            let img = Img { w, h: px, px: rgba };
            let t = texture(&self.creators[v.c], &img)?;
            self.glyphs.insert((glyph, px), t);
        }
        Ok((glyph, px))
    }

    /// The power menu: both screens dimmed, its choices in a box on the touch screen, `row` lit.
    /// The bottom screen while a port plays on the top one: what is playing, and how to leave
    /// it (PortMaster's runner quits on either pair; holding power is DSlot's power menu).
    pub fn draw_port_help(&mut self, name: &str) -> Result<(), String> {
        let v = self.bot;
        self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
        self.white[v.c].set_color_mod(16, 18, 26);
        self.white[v.c].set_alpha_mod(255);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(0.0, 0.0, SW, SH))?;
        self.text(
            v,
            "Playing on the top screen",
            (24.0, false),
            (SW / 2.0, 190.0),
            0.5,
            DIM,
        )?;
        let size = self.fonts.fit(name, 52.0, 28.0, SW - 120.0, true);
        self.text(v, name, (size, true), (SW / 2.0, 232.0), 0.5, WHITE)?;
        let rows = [
            ("Select + Start", "quit"),
            ("Menu + Start", "quit"),
            ("Hold power", "the power menu"),
        ];
        let mut y = 400.0;
        for (keys, does) in rows {
            self.text(v, keys, (30.0, true), (SW / 2.0 - 20.0, y), 1.0, GOLD)?;
            self.text(v, does, (30.0, false), (SW / 2.0 + 20.0, y), 0.0, WHITE)?;
            y += 56.0;
        }
        Ok(())
    }

    /// A game came straight back: why, over both screens dimmed, in a box on the touch screen,
    /// until any button is pressed; with the bug report's state under it, where one can be sent.
    pub fn draw_notice(&mut self, shown: &NoticeShown) -> Result<(), String> {
        for v in [self.top, self.bot] {
            self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
            self.shade(v, 0.75)?;
        }
        let v = self.bot;
        let lines = crate::update::wrap(shown.text, NOTICE_WIDTH);
        let step = 30.0;
        let outcome_h = match (shown.outcome.is_some(), shown.code.is_some()) {
            (false, _) => 0.0,
            (true, false) => 56.0,
            (true, true) => 128.0,
        };
        let height = 150.0 + lines.len().min(NOTICE_LINES) as f32 * step + outcome_h;
        let (x, width) = (40.0, SW - 80.0);
        let top = (SH - height) / 2.0;
        self.white[v.c].set_alpha_mod(255);
        self.white[v.c].set_color_mod(PANEL.r, PANEL.g, PANEL.b);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, top, width, height))?;
        self.text(
            v,
            shown.title,
            (28.0, true),
            (SW / 2.0, top + 22.0),
            0.5,
            RED,
        )?;
        let mut y = top + 74.0;
        for line in lines.iter().take(NOTICE_LINES) {
            self.text(v, line, (20.0, false), (x + 24.0, y), 0.0, WHITE)?;
            y += step;
        }
        if let Some(outcome) = &shown.outcome {
            y += 16.0;
            self.text(v, outcome, (22.0, true), (SW / 2.0, y), 0.5, GOLD)?;
            y += 34.0;
            if let Some(code) = shown.code {
                self.text(v, code, (56.0, true), (SW / 2.0, y), 0.5, WHITE)?;
            }
        }
        let hint = if shown.hint.is_empty() {
            String::new()
        } else {
            format!(
                "The whole log: {}   \u{b7}   {}",
                crate::platform::LOG_HINT,
                shown.hint
            )
        };
        self.text(
            v,
            &hint,
            (18.0, false),
            (SW / 2.0, top + height - 40.0),
            0.5,
            FAINT,
        )
    }

    pub fn draw_power(&mut self, labels: &[&str], row: usize) -> Result<(), String> {
        for v in [self.top, self.bot] {
            self.canvases[v.c].set_clip_rect(v.rect(0.0, 0.0, SW, SH));
            self.shade(v, 0.75)?;
        }
        let v = self.bot;
        let (step, width) = (power::STEP, power::WIDTH);
        let x = SW / 2.0 - width / 2.0;
        let top = power::row_y(0) - step / 2.0 - 64.0;
        let height = labels.len() as f32 * step + 84.0;
        self.white[v.c].set_alpha_mod(255);
        self.white[v.c].set_color_mod(PANEL.r, PANEL.g, PANEL.b);
        self.canvases[v.c].copy(&self.white[v.c], None, v.rect(x, top, width, height))?;
        self.text(v, "Power", (24.0, true), (SW / 2.0, top + 18.0), 0.5, FAINT)?;
        for (r, label) in labels.iter().enumerate() {
            let y = power::row_y(r);
            let lit = r == row;
            if lit {
                self.white[v.c].set_color_mod(WHITE.r, WHITE.g, WHITE.b);
                let band = v.rect(x + 16.0, y - step / 2.0 + 4.0, width - 32.0, step - 8.0);
                self.canvases[v.c].copy(&self.white[v.c], None, band)?;
            }
            let ink = if lit { Color::RGB(20, 23, 30) } else { WHITE };
            self.text(v, label, (30.0, true), (SW / 2.0, y - 19.0), 0.5, ink)?;
        }
        Ok(())
    }

    /// A page with no games: where they were looked for.
    pub fn empty(&mut self, searched: &[PathBuf], console: Console) -> Result<(), String> {
        let (top, bot) = (self.top, self.bot);
        let title = format!("No {} games found", console.label());
        let hint = format!(
            "Copy .{} files (or .zip) to one of the folders below.",
            console.ext()
        );
        self.text(top, &title, (48.0, true), (SW / 2.0, 320.0), 0.5, WHITE)?;
        self.text(top, &hint, (24.0, false), (SW / 2.0, 392.0), 0.5, DIM)?;
        let mut y = 300.0;
        for dir in searched {
            self.text(
                bot,
                &dir.to_string_lossy(),
                (24.0, false),
                (SW / 2.0, y),
                0.5,
                DIM,
            )?;
            y += 38.0;
        }
        self.text(
            bot,
            "L / R  page   \u{b7}   Select + Start  exit",
            (18.0, false),
            (SW / 2.0, 636.0),
            0.5,
            FAINT,
        )
    }
}

/// A page of the shelf as it is drawn this frame.
/// A blade in the system picker: its page, how many games it has and when one was last
/// played, and the game whose art it shows (its box, and its cart when it is chosen).
pub struct Blade {
    pub console: Console,
    pub games: usize,
    pub last: Option<String>,
    pub game: Option<usize>,
    /// Whether that game has box art of its own (in `Imgs/`), rather than a printed box.
    pub art: bool,
}

/// What a system's card says of the player's own games for it.
pub struct Yours {
    pub games: usize,
    pub secs: u64,
    /// The game played longest (or for apps, the one used last), by name.
    pub most: Option<String>,
    /// What `most` is: "Most played", "Last used".
    pub most_is: &'static str,
}

/// `c` darkened to `k` of itself.
fn dim_by(c: Color, k: f32) -> Color {
    Color::RGB(
        (c.r as f32 * k) as u8,
        (c.g as f32 * k) as u8,
        (c.b as f32 * k) as u8,
    )
}

/// A system's colour: the field its blade's art sits on, and all a blade shows without art.
fn console_colour(console: Console) -> Color {
    match console {
        Console::Ds => Color::RGB(58, 68, 92),
        Console::Gba => Color::RGB(76, 58, 128),
        Console::Gb => Color::RGB(104, 116, 78),
        Console::Gbc => Color::RGB(118, 58, 108),
        Console::Snes => Color::RGB(104, 96, 136),
        Console::Nes => Color::RGB(136, 52, 52),
        Console::MegaDrive => Color::RGB(44, 44, 50),
        Console::GameGear => Color::RGB(70, 72, 82),
        Console::Lynx => Color::RGB(50, 66, 70),
        Console::GameWatch => Color::RGB(146, 116, 58),
        Console::Fds => Color::RGB(150, 120, 40),
        Console::MasterSystem => Color::RGB(36, 36, 44),
        Console::Sg1000 => Color::RGB(120, 44, 44),
        Console::PcEngine => Color::RGB(96, 98, 104),
        Console::PcEngineCd => Color::RGB(64, 72, 96),
        Console::PlayStation => Color::RGB(110, 112, 122),
        Console::NeoGeoPocket => Color::RGB(40, 70, 120),
        Console::WonderSwan => Color::RGB(80, 100, 120),
        Console::Ports => Color::RGB(150, 70, 48),
        Console::Apps => Color::RGB(46, 88, 90),
    }
}

#[derive(Clone, Copy)]
pub struct Page<'a> {
    /// Every game, shown or not.
    pub games: &'a [Game],
    /// The games on show, in order, as indices into `games`.
    pub list: &'a [usize],
    /// The selected game's place in `list`.
    pub sel: usize,
    pub console: Console,
    /// The line at the top of the touch screen, right of the page tabs.
    pub heading: &'a str,
    /// The page tabs' strip, which the heading is beside.
    pub strip: Strip,
    /// How far the screens have faded to black as a game starts.
    pub fade: f32,
}

/// Lets go of a texture. The `unsafe_textures` build of SDL2 leaves that to DSlot, which
/// makes them for one renderer and drops them before it.
pub(super) fn destroy(t: Texture) {
    // SAFETY: every texture is destroyed once, before the renderer it was made by.
    unsafe { t.destroy() }
}

pub(super) fn texture(tc: &TextureCreator<WindowContext>, img: &Img) -> Result<Texture, String> {
    let mut t = tc
        .create_texture_static(PixelFormatEnum::ABGR8888, img.w as u32, img.h as u32)
        .map_err(|e| e.to_string())?;
    t.update(None, &img.px, img.w * 4)
        .map_err(|e| e.to_string())?;
    t.set_blend_mode(BlendMode::Blend);
    Ok(t)
}

/// The status bar with the slot recess cut out of its top edge. For slot-2 (GBA carts) the
/// recess is the long, shallow opening along a DS Lite's front edge, with the dark mouth of the
/// slot across its floor.
/// A menu row as nibkit's layout takes it.
fn menu_row(row: &super::menu::Row) -> nibkit::menu::Row {
    use nibkit::menu::Value as V;
    let value = match &row.value {
        Value::Text(t) => V::Text(t.clone()),
        Value::Inherited(t) => V::Inherited(t.clone()),
        Value::Stars(n) => V::Stars(*n),
        Value::Status { text, glyphs } => V::Status {
            text: text.clone(),
            glyphs: glyphs.clone(),
        },
        Value::Slider(p) => V::Slider((*p).clamp(0, 100) as u8),
        Value::Prompt { text, warning } => V::Prompt {
            text: text.clone(),
            warning: *warning,
        },
    };
    let laid_out = nibkit::menu::Row::new(row.label.clone(), value);
    match &row.source {
        Some(note) => laid_out.with_note(note.clone()),
        None => laid_out,
    }
}

/// Which status bar a console's page has: the DS slot, or slot-2 opened to its carts' width.
fn bar_for(console: Console) -> usize {
    match console {
        Console::Ds => 0,
        // A disc goes into a slot a little wider than itself, as the narrow carts do.
        Console::Gb
        | Console::Gbc
        | Console::GameGear
        | Console::Lynx
        | Console::PlayStation
        | Console::PcEngineCd => 2,
        Console::Gba
        | Console::Snes
        | Console::Nes
        | Console::MegaDrive
        | Console::GameWatch
        | Console::Apps
        | Console::Ports
        | Console::Fds
        | Console::MasterSystem
        | Console::Sg1000
        | Console::PcEngine
        | Console::NeoGeoPocket
        | Console::WonderSwan => 1,
    }
}

/// How much wider slot-2's opening is than the cart that goes into it.
const SLOT2_FIT: f32 = 600.0 / 560.0;

/// The status bar with a slot in its top edge: the DS's, or slot-2's opening for carts
/// `cart_w` wide (in screen units), snug as on the real thing.
pub fn slot_bar(s: f32, slot: Slot, cart_w: f32) -> Img {
    let (w, h) = ((SW * s) as usize, (BAR_H * s) as usize);
    let cx = w as f32 / 2.0;
    let mut img = Img::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let px = match slot {
                Slot::One => {
                    let (rx, ry) = (215.0 * s, 54.0 * s);
                    let d = ((fx - cx) / rx).hypot(fy / ry);
                    let a = (0.5 + (d - 1.0) * ry).clamp(0.0, 1.0);
                    [28, 33, 44, (a * 255.0) as u8]
                }
                Slot::Two => {
                    // A rounded notch a little wider than the cart (600 for the GBA's 560) and
                    // 30 deep: signed distance to its edge.
                    let (hw, depth, r) = (cart_w * SLOT2_FIT / 2.0 * s, 30.0 * s, 12.0 * s);
                    let (qx, qy) = ((fx - cx).abs() - hw + r, fy - depth + r);
                    let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r;
                    let a = (0.5 + d).clamp(0.0, 1.0);
                    // The slot's mouth: a dark line just under the notch's floor.
                    let mouth =
                        (fx - cx).abs() < hw - 16.0 * s && (depth..depth + 7.0 * s).contains(&fy);
                    if mouth {
                        [8, 9, 12, 255]
                    } else {
                        [28, 33, 44, (a * 255.0) as u8]
                    }
                }
            };
            img.px[(y * w + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    img
}
