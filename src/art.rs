//! Builds each game's cart and box. Real artwork wins when it exists; otherwise both are
//! generated from the ROM's own icon, title and game code.

mod disc;
mod finish;
mod shells;
mod studio;

use crate::gfx::{load_png, Fonts, Img, Rgba};
use crate::rom::{icon_colour, Console, Game};
use crate::settings::{Case, SnesCart};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

pub const CART_W: usize = 448;
pub const CART_H: usize = 480;
/// A GBA cartridge, 57 x 35 mm against the DS card's 33 x 35: wider and a little shorter.
pub const GBA_CART_W: usize = 640;
pub const GBA_CART_H: usize = 370;
/// A Game Boy (and Game Boy Color) cart, 57 x 65 mm: as wide as a GBA cart and nearly twice
/// as tall.
pub const GB_CART_W: usize = 480;
pub const GB_CART_H: usize = 547;
/// A GBA box is cardboard, and about square; the box art for it is too.
pub const GBA_BOX: (usize, usize) = (452, 452);
/// SNES/Super Famicom carts and the landscape cardboard box used for the SNES shelf.
pub const SNES_CART: (usize, usize) = shells::SNES_SIZE;
pub const SNES_BOX: (usize, usize) = (520, 364);
/// NES front-loading cartridge and its portrait cardboard box.
pub const NES_CART: (usize, usize) = (480, 560);
pub const NES_BOX: (usize, usize) = (400, 560);
pub const MD_CART: (usize, usize) = shells::md::SIZE;
pub const GG_CART: (usize, usize) = shells::gg::SIZE;
/// The Lynx cart, front on: nearly square, as a real one is (289 x 301).
pub const LYNX_CART: (usize, usize) = (560, 583);
const SEGA_BOX: (usize, usize) = (400, 560);
const LYNX_BOX: (usize, usize) = (400, 520);
const GW_BOX: (usize, usize) = (480, 300);
/// A DS case seen from the front, 137 x 125 mm, and the plastic showing around the cover.
pub const CASE_SIZE: (usize, usize) = (482, 440);
const CASE_EDGE: usize = 7;
const CASE_RADIUS: f32 = 10.0;
/// Plastic and the lighter rim on its edge: the black case, then the clear one.
const CASE_BLACK: [Rgba; 2] = [[24, 26, 31, 255], [70, 74, 84, 255]];
const CASE_CLEAR: [Rgba; 2] = [[198, 204, 212, 225], [244, 246, 250, 245]];

/// The cart's plastic is drawn on a 336 x 360 grid with the sticker's window at x, y, w, h.
/// Those are the proportions GameTDB uses for its DS cart and label images, so labels made
/// for one fit the other.
const FRAME_SIZE: (f32, f32) = (336.0, 360.0);
const FRAME_STICKER: (f32, f32, f32, f32) = (32.0, 20.0, 272.0, 296.0);

/// A DS card's front, laid out on that grid. The body is a pixel narrower than the grid on each
/// side. A groove runs round its lower edges, setting off a narrow lip: it starts at a ledge on
/// the left edge, follows the big bottom-left corner, runs along the bottom and turns up the
/// right side to a second ledge.
mod card {
    pub const BODY: (f32, f32, f32, f32) = (1.0, 0.0, 334.0, 360.0);
    /// Corner radii: top-left, top-right, bottom-right, bottom-left. The last is much the roundest.
    pub const CORNERS: [f32; 4] = [9.0, 9.0, 9.0, 34.0];

    /// The groove. Left: ledge height and the x it runs down at. Its turn: centre and radius.
    /// Bottom: its height. Right: the x it runs up at, and that ledge's height.
    pub const LEFT: (f32, f32) = (308.7, 5.5);
    pub const TURN: (f32, f32, f32) = (35.6, 323.4, 30.1);
    pub const BOTTOM: f32 = 353.6;
    pub const RIGHT: (f32, f32) = (330.2, 329.3);

    /// The well the sticker sits in, its corner radius, and how much of its bottom-left
    /// corner is cut off at 45 degrees. The window inside it is cut the same way.
    pub const WELL: (f32, f32, f32, f32) = (24.5, 12.5, 286.0, 310.0);
    pub const WELL_RADIUS: f32 = 11.0;
    pub const WELL_CUT: f32 = 21.5;
    pub const WINDOW: (f32, f32, f32, f32) = (31.5, 19.5, 272.0, 296.5);
    pub const WINDOW_RADIUS: f32 = 12.0;
    pub const WINDOW_CUT: f32 = 23.5;
    /// The arrow pointing into the slot: centre, top, width, height.
    pub const ARROW: (f32, f32, f32, f32) = (167.5, 327.2, 40.0, 14.8);

    /// Greys, as brightness; the plastic has a faint green cast (see `Shell::load`).
    pub const FACE: f32 = 72.0;
    pub const WELL_FACE: f32 = 56.0;
    pub const WELL_RIM: f32 = 46.0;
    pub const ARROW_FACE: f32 = 58.0;
    pub const GROOVE: f32 = 22.0;
    /// The lip catches the light on the left and is in shade along the bottom.
    pub const LIP: (f32, f32, f32) = (84.0, 67.0, 71.0);
}

/// The GBA cartridge's front, laid out on a 1820 x 1052 grid measured from a photo of a real one
/// (Gekkio's GB hardware database, AGB-B8ME). The grip along the top is the full width, with large
/// rounded corners; below it the body steps in a little on each side. The grip's face is a
/// lighter band whose lower edge runs straight across and turns up toward the corners. The
/// console's name, embossed under it on the real thing, is left out. Then the label's well, and
/// a triangle pressed into the face below it, pointing into the slot.
mod gba {
    pub const GRID: (f32, f32) = (1820.0, 1052.0);
    pub const GRIP: (f32, f32, f32, f32) = (2.0, 0.0, 1816.0, 200.0);
    pub const GRIP_CORNERS: [f32; 4] = [52.0, 52.0, 8.0, 8.0];
    pub const BODY: (f32, f32, f32, f32) = (44.0, 150.0, 1732.0, 902.0);
    pub const BODY_CORNERS: [f32; 4] = [0.0, 0.0, 30.0, 30.0];
    /// The grip's face: its lower edge, and where it turns up to meet the corners.
    pub const FACE_EDGE: f32 = 146.0;
    pub const FACE_TURN: [(f32, f32); 2] = [(232.0, 146.0), (46.0, 58.0)];
    pub const WELL: (f32, f32, f32, f32) = (239.0, 215.0, 1342.0, 716.0);
    pub const WELL_RADIUS: f32 = 30.0;
    pub const WINDOW: (f32, f32, f32, f32) = (253.0, 229.0, 1314.0, 688.0);
    pub const WINDOW_RADIUS: f32 = 22.0;
    /// The triangle: the ends of its top edge and its point.
    pub const VEE: [(f32, f32); 3] = [(745.0, 966.0), (910.0, 1024.0), (1075.0, 966.0)];
    /// Brightness of the charcoal plastic, the grip's face, the well's floor and rim, and the
    /// inside of the triangle.
    pub const FACE: f32 = 76.0;
    pub const GRIP_FACE: f32 = 88.0;
    pub const WELL_FACE: f32 = 60.0;
    pub const WELL_RIM: f32 = 44.0;
    pub const VEE_FACE: f32 = 58.0;
}

/// The Game Boy cart's front, the grey DMG one and the black and coloured ones on the same mould,
/// on a 570 x 650 grid (tenths of a millimetre) measured from photos of real ones (Gekkio's GB
/// hardware database: DMG-TRA-1, Tetris, square on; DMG-ALZP-0). A notch is cut from the top
/// right corner. Along the top, five ribs at each side make the grip, and between them a plate is
/// pressed in (the console's name, embossed in it on the real thing, is left out). Below, the
/// label's well, and a triangle pressed in under it, pointing into the slot.
mod gb {
    pub const GRID: (f32, f32) = (570.0, 650.0);
    pub const BODY: (f32, f32, f32, f32) = (0.0, 0.0, 570.0, 650.0);
    pub const CORNERS: [f32; 4] = [14.0, 14.0, 16.0, 16.0];
    /// The notch: its left edge and its depth.
    pub const NOTCH: (f32, f32) = (522.0, 31.0);
    /// The ribs: the grooves' heights, and how far in from each side they run.
    pub const GROOVES: [f32; 5] = [53.0, 73.0, 93.0, 113.0, 133.0];
    pub const RIBS: (f32, f32) = (60.0, 515.0);
    pub const PLATE: (f32, f32, f32, f32) = (67.0, 39.0, 448.0, 118.0);
    pub const PLATE_RADIUS: f32 = 45.0;
    pub const WELL: (f32, f32, f32, f32) = (61.0, 180.0, 452.0, 386.0);
    pub const WELL_RADIUS: f32 = 10.0;
    pub const WINDOW: (f32, f32, f32, f32) = (66.0, 189.0, 442.0, 372.0);
    pub const WINDOW_RADIUS: f32 = 7.0;
    /// The triangle: the ends of its top edge and its point.
    pub const VEE: [(f32, f32); 3] = [(220.0, 583.0), (282.0, 631.0), (344.0, 583.0)];
    /// Brightness of the face, the plate's floor, the well's floor and rim, and the triangle's.
    pub const FACE: f32 = 186.0;
    pub const PLATE_FACE: f32 = 172.0;
    pub const WELL_FACE: f32 = 176.0;
    pub const WELL_RIM: f32 = 120.0;
    pub const VEE_FACE: f32 = 172.0;
}

/// The Game Boy Color cart's front, for the games that only play on a Color: clear plastic on a
/// mould of its own (CGB-AHYE-0, Super Mario Bros. Deluxe). No notch, plate or grip on its face;
/// the label sits lower, with the board showing through above and below it: its chips, and
/// the contacts along the bottom.
mod gbc {
    pub const CORNERS: [f32; 4] = [18.0, 18.0, 18.0, 18.0];
    pub const WELL: (f32, f32, f32, f32) = (68.0, 205.0, 432.0, 390.0);
    pub const WELL_RADIUS: f32 = 9.0;
    pub const WINDOW: (f32, f32, f32, f32) = (73.0, 210.0, 422.0, 380.0);
    pub const WINDOW_RADIUS: f32 = 7.0;
    /// Ribs on the edges at each side near the top: from, to, and how many.
    pub const RIBS: (f32, f32, f32) = (51.0, 141.0, 6.0);
    /// The board behind the plastic, its chips, and the contacts along its bottom edge.
    pub const BOARD: (f32, f32, f32, f32) = (32.0, 22.0, 506.0, 618.0);
    pub const CHIPS: [(f32, f32, f32, f32); 2] =
        [(115.0, 72.0, 100.0, 56.0), (300.0, 92.0, 66.0, 40.0)];
    pub const CONTACTS: (f32, f32, f32, f32) = (45.0, 612.0, 480.0, 30.0);
    pub const PINS: usize = 32;
    /// The plastic: its smoky blue tint and how much of what's behind it shows, then the board,
    /// the chips and the contacts.
    pub const PLASTIC: [f32; 3] = [164.0, 168.0, 192.0];
    pub const CLEAR: f32 = 0.55;
    pub const BOARD_COLOUR: [f32; 3] = [34.0, 92.0, 58.0];
    pub const CHIP_COLOUR: [f32; 3] = [22.0, 22.0, 26.0];
    pub const GOLD: [f32; 3] = [196.0, 158.0, 72.0];
}

/// A cart's plastic, and the window the sticker shows through, at cart size, grown by a pixel
/// so nothing shows through at the edge.
pub struct Frame {
    img: Img,
    sticker: (i32, i32, i32, i32),
}

/// Every console's cart shell, by `Console as usize`. The DS card's is drawn at start; the others
/// the first time one of their carts is wanted, so a start on the DS page doesn't wait for them.
pub struct Shell {
    frames: [OnceLock<Frame>; Console::COUNT],
    /// The SNES's other shape: `frames` has the US one.
    famicom: OnceLock<Frame>,
    /// Shells in a colour and finish of a `cart_shell.txt`'s, made the first time one is wanted.
    styled: Mutex<HashMap<(Console, studio::Style), Arc<Frame>>>,
}

/// What a page's artwork looks like besides its own pictures: the case its boxes come in, and
/// (on the SNES page) which shape its carts are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Looks {
    pub case: Case,
    pub snes: SnesCart,
}

impl Looks {
    /// Whether `game`'s box is the black case.
    pub fn black(&self, game: &Game) -> bool {
        self.case.black(&game.code)
    }

    /// Whether `game`'s SNES cart is the US shape: as set, or as the game's region was (by its
    /// header, else its file's name), and with neither, as the system's locale is.
    pub fn us_snes(&self, game: &Game) -> bool {
        match self.snes {
            SnesCart::Us => true,
            SnesCart::Famicom => false,
            SnesCart::Region => crate::rom::snes_north_american(&game.path, &game.stem)
                .unwrap_or_else(north_american_locale),
        }
    }
}

/// Whether the system's locale is the US's or Canada's, where SNES carts had the US shape.
fn north_american_locale() -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .is_some_and(|v| v.contains("_US") || v.contains("_CA"))
}

/// How a console's shell is drawn. A console without one has no carts, and no shelf of them.
fn shell_of(console: Console) -> Option<fn() -> Frame> {
    match console {
        Console::Ds => Some(ds_frame),
        Console::Gba => Some(gba_frame),
        Console::Gb => Some(gb_frame),
        Console::Gbc => Some(gbc_frame),
        Console::Snes => Some(shells::snes_us_frame),
        Console::Nes => Some(nes_frame),
        Console::MegaDrive => Some(shells::md_frame),
        Console::GameGear => Some(shells::gg_frame),
        Console::Lynx => Some(lynx_frame),
        // A Game & Watch unit is the game itself, a disk or a disc is no cart, and apps are
        // programs. The Master System's, PC Engine's, Neo Geo Pocket's and WonderSwan's carts
        // have no shell drawn yet.
        Console::GameWatch
        | Console::Apps
        | Console::Ports
        | Console::Fds
        | Console::MasterSystem
        | Console::Sg1000
        | Console::PcEngine
        | Console::PcEngineCd
        | Console::PlayStation
        | Console::NeoGeoPocket
        | Console::WonderSwan => None,
    }
}

/// Whether DSlot draws this console's carts. Where it doesn't (a Game & Watch unit, a disk or
/// a disc, the apps, or a cart it has no shell for yet), its page shows boxes alone.
pub fn has_shell(console: Console) -> bool {
    shell_of(console).is_some() || disc::is_disc(console)
}

/// The slot a console's carts go into: the DS's own, at the back of a DS Lite, or slot-2 at the
/// front, which takes GBA carts (and on a GBA, GB and GBC ones).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    One,
    Two,
}

pub fn slot(console: Console) -> Slot {
    match console {
        Console::Ds => Slot::One,
        Console::Gba
        | Console::Gb
        | Console::Gbc
        | Console::Snes
        | Console::Nes
        | Console::MegaDrive
        | Console::GameGear
        | Console::Lynx
        | Console::GameWatch
        | Console::Apps
        | Console::Ports
        | Console::Fds
        | Console::MasterSystem
        | Console::Sg1000
        | Console::PcEngine
        | Console::PcEngineCd
        | Console::PlayStation
        | Console::NeoGeoPocket
        | Console::WonderSwan => Slot::Two,
    }
}

/// Signed distance to a rounded rectangle, negative inside. Radii as in `card::CORNERS`.
fn rounded(x: f32, y: f32, rect: (f32, f32, f32, f32), r: [f32; 4]) -> f32 {
    let (dx, dy) = (x - rect.0 - rect.2 / 2.0, y - rect.1 - rect.3 / 2.0);
    let rad = match (dx < 0.0, dy < 0.0) {
        (true, true) => r[0],
        (false, true) => r[1],
        (false, false) => r[2],
        (true, false) => r[3],
    };
    let (qx, qy) = (dx.abs() - rect.2 / 2.0 + rad, dy.abs() - rect.3 / 2.0 + rad);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad
}

/// The same with the bottom-left corner cut off at 45 degrees, `cut` along each side.
fn clipped(x: f32, y: f32, rect: (f32, f32, f32, f32), r: f32, cut: f32) -> f32 {
    let corner = (cut - (x - rect.0) - (rect.1 + rect.3 - y)) / std::f32::consts::SQRT_2;
    rounded(x, y, rect, [r; 4]).max(corner)
}

/// Distance from a point to a line segment.
fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * vx + (y - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    (x - a.0 - vx * t).hypot(y - a.1 - vy * t)
}

/// Distance to the groove, and how far along it the nearest point is: 0 on the left side,
/// 1 along the bottom, 2 up the right.
fn groove(x: f32, y: f32) -> (f32, f32) {
    use card::*;
    let right = BODY.0 + BODY.2;
    let (cx, cy, r) = TURN;
    let mut best = (f32::MAX, 0.0);
    let mut take = |d: f32, along: f32| {
        if d < best.0 {
            best = (d, along);
        }
    };
    take(segment(x, y, (BODY.0, LEFT.0), (LEFT.1, LEFT.0)), 0.0);
    take(segment(x, y, (LEFT.1, LEFT.0), (LEFT.1, cy)), 0.0);
    // The quarter turn, where the point is in its quadrant.
    if x <= cx && y >= cy {
        let turned = (y - cy).atan2(cx - x) / std::f32::consts::FRAC_PI_2;
        take(((x - cx).hypot(y - cy) - r).abs(), turned);
    }
    take(segment(x, y, (cx, BOTTOM), (RIGHT.0 - 2.0, BOTTOM)), 1.0);
    take(
        segment(x, y, (RIGHT.0 - 2.0, BOTTOM), (RIGHT.0, BOTTOM - 2.0)),
        1.5,
    );
    take(
        segment(x, y, (RIGHT.0, BOTTOM - 2.0), (RIGHT.0, RIGHT.1)),
        2.0,
    );
    take(segment(x, y, (RIGHT.0, RIGHT.1), (right, RIGHT.1)), 2.0);
    best
}

/// Whether a point is on the lip, outside the groove.
fn on_lip(x: f32, y: f32) -> bool {
    use card::*;
    let (cx, cy, r) = TURN;
    (x < LEFT.1 && y > LEFT.0)
        || y > BOTTOM
        || (x > RIGHT.0 && y > RIGHT.1)
        || (x < cx && y > cy && (x - cx).hypot(y - cy) > r)
}

impl Shell {
    pub fn load() -> Shell {
        let shell = Shell {
            frames: Default::default(),
            famicom: OnceLock::new(),
            styled: Mutex::default(),
        };
        shell.frame(Console::Ds);
        shell
    }

    fn frame(&self, console: Console) -> &Frame {
        let draw = shell_of(console)
            .unwrap_or_else(|| unreachable!("no cart shell for {console:?}, so no page"));
        self.frames[console as usize].get_or_init(draw)
    }

    /// The shell for a cart of `console`'s: for the SNES, the US shape or the Super Famicom's.
    fn cart_frame(&self, console: Console, us: bool) -> &Frame {
        if console == Console::Snes && !us {
            return self.famicom.get_or_init(shells::snes_famicom_frame);
        }
        self.frame(console)
    }

    /// `console`'s shell as `style` has it: the outline it names (the Game Boy's two), in its
    /// colour and finish.
    fn styled(&self, console: Console, style: studio::Style) -> Arc<Frame> {
        use studio::Outline;
        if let Some(frame) = self.styled.lock().unwrap().get(&(console, style)) {
            return frame.clone();
        }
        // Drawn without the lock held: another worker may be drawing a cart meanwhile.
        let rounded;
        let base = match (console, style.outline) {
            (Console::Gb | Console::Gbc, Outline::Notched) => self.frame(Console::Gb),
            (Console::Gb | Console::Gbc, Outline::Rounded) | (Console::Gbc, Outline::Auto) => {
                rounded = gbc_shell(false);
                &rounded
            }
            (console, _) => self.frame(console),
        };
        let frame = Arc::new(finish::restyle(base, console, style.colour, style.finish));
        self.styled
            .lock()
            .unwrap()
            .insert((console, style), frame.clone());
        frame
    }
}

/// The size a console's carts are drawn at, in pixels.
pub fn cart_size(console: Console) -> (usize, usize) {
    match console {
        Console::Ds => (CART_W, CART_H),
        Console::Gba => (GBA_CART_W, GBA_CART_H),
        Console::Snes => SNES_CART,
        Console::Nes => NES_CART,
        Console::MegaDrive => MD_CART,
        Console::GameGear => GG_CART,
        Console::Lynx => LYNX_CART,
        Console::PlayStation | Console::PcEngineCd => disc::SIZE,
        // A page without carts is a grid or a list: their size is asked for, never drawn.
        Console::GameWatch
        | Console::Apps
        | Console::Ports
        | Console::Fds
        | Console::MasterSystem
        | Console::Sg1000
        | Console::PcEngine
        | Console::NeoGeoPocket
        | Console::WonderSwan => (GBA_CART_W, GBA_CART_H),
        Console::Gb | Console::Gbc => (GB_CART_W, GB_CART_H),
    }
}

/// The size of a console's box on the top screen.
pub fn box_size(console: Console) -> (usize, usize) {
    match console {
        Console::Ds => CASE_SIZE,
        Console::Snes => SNES_BOX,
        Console::Nes => NES_BOX,
        Console::MegaDrive | Console::GameGear | Console::MasterSystem | Console::Sg1000 => {
            SEGA_BOX
        }
        Console::Fds => NES_BOX,
        Console::Lynx => LYNX_BOX,
        // A Game & Watch box shows the unit, landscape, about as wide as a Wide Screen one.
        Console::GameWatch | Console::Ports => GW_BOX,
        // GB and GBC boxes are card and about square too, as the PC Engine's, the Neo Geo
        // Pocket's and the WonderSwan's are near enough; a CD's case is square.
        Console::Gba
        | Console::Gb
        | Console::Gbc
        | Console::Apps
        | Console::PcEngine
        | Console::PcEngineCd
        | Console::PlayStation
        | Console::NeoGeoPocket
        | Console::WonderSwan => GBA_BOX,
    }
}

fn ds_frame() -> Frame {
    {
        use card::*;
        let (fx, fy) = (CART_W as f32 / FRAME_SIZE.0, CART_H as f32 / FRAME_SIZE.1);
        let mix = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
        // Distances are in grid units; this turns one into coverage at the edge of a shape.
        let cover = |d: f32| (0.5 - d * fx).clamp(0.0, 1.0);
        // A soft line of the given half-width centred on distance zero.
        let line = |d: f32, half: f32| (1.0 - d.abs() / half).clamp(0.0, 1.0);

        let mut frame = Img::new(CART_W, CART_H);
        for py in 0..CART_H {
            for px in 0..CART_W {
                let (x, y) = ((px as f32 + 0.5) / fx, (py as f32 + 0.5) / fy);
                let outline = rounded(x, y, BODY, CORNERS);

                // The face, lit from the top left: a shade darker toward the right.
                let mut c = FACE * (1.02 - 0.05 * x / FRAME_SIZE.0);
                let (dist, along) = groove(x, y);
                if on_lip(x, y) {
                    c = if along <= 1.0 {
                        mix(LIP.0, LIP.1, along)
                    } else {
                        mix(LIP.1, LIP.2, along - 1.0)
                    };
                } else if along < 1.0 {
                    // The face rises a little brighter just inside the groove on the lit side.
                    c += 6.0 * line(dist - 2.6, 1.3) * (1.0 - along);
                }
                // The groove is deepest on the sides and shallower along the bottom.
                let depth = if along <= 1.0 {
                    mix(1.0, 0.72, along)
                } else {
                    mix(0.72, 1.0, (along - 1.0) * 2.0)
                };
                c = mix(c, GROOVE, depth * line(dist, 1.25));

                // The well: a bright edge on the face around it, a dark rim just inside.
                let well = clipped(x, y, WELL, WELL_RADIUS, WELL_CUT);
                c += 5.0 * line(well - 1.8, 1.6);
                let rim = if y < WELL.1 + WELL_RADIUS { 0.35 } else { 1.0 };
                let inside = mix(WELL_FACE, WELL_RIM, rim * line(well + 1.3, 1.5));
                c = mix(c, inside, cover(well));

                let (ax, ay) = ((x - ARROW.0).abs(), y - ARROW.1);
                let arrow = (ax - ARROW.2 / 2.0 * (1.0 - ay / ARROW.3)).max(-ay);
                c = mix(c, ARROW_FACE, cover(arrow * 0.8));

                // The outer edge falls away into shadow, except on the left where it is lit.
                let edge = -outline;
                if x < BODY.0 + 4.0 && y > CORNERS[0] && y < TURN.1 {
                    c *= if edge < 1.0 {
                        0.84
                    } else {
                        mix(1.08, 1.0, (edge - 2.2) / 1.2)
                    };
                } else {
                    c *= mix(0.42, 1.0, (edge + 0.2) / 2.6);
                }

                // Matte plastic: a fine grain.
                let grain = (px as u32)
                    .wrapping_mul(0x9E3779B1)
                    .wrapping_add((py as u32).wrapping_mul(0x85EBCA6B));
                let grain = ((grain ^ (grain >> 15)).wrapping_mul(0x2C1B3C6D) >> 28) as f32 - 7.5;
                c *= 1.0 + grain * 0.004;

                let window = clipped(x, y, WINDOW, WINDOW_RADIUS, WINDOW_CUT);
                let alpha = cover(outline) * (1.0 - cover(window));
                let i = (py * CART_W + px) * 4;
                // Slightly more green and blue than red.
                for (k, tint) in [0.97, 1.02, 1.02].into_iter().enumerate() {
                    frame.px[i + k] = (c * tint).clamp(0.0, 255.0) as u8;
                }
                frame.px[i + 3] = (alpha * 255.0) as u8;
            }
        }

        let (x, y, w, h) = FRAME_STICKER;
        let sticker = (
            (x * fx) as i32 - 1,
            (y * fy) as i32 - 1,
            (w * fx).ceil() as i32 + 2,
            (h * fy).ceil() as i32 + 2,
        );
        Frame {
            img: frame,
            sticker,
        }
    }
}

/// Grey front-loading NES cartridge with a tall label, side grip and connector recess.
fn nes_frame() -> Frame {
    let (w, h) = NES_CART;
    let mut img = Img::new(w, h);
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let body = rounded(x, y, (2.0, 2.0, 476.0, 556.0), [8.0; 4]);
            let window = rounded(x, y, (104.0, 44.0, 308.0, 396.0), [3.0; 4]);
            let mut c = 178.0 - 10.0 * x / w as f32 - 8.0 * y / h as f32;
            if (14.0..82.0).contains(&x) && (20.0..492.0).contains(&y) {
                let rib = y % 14.0;
                c += if rib < 2.0 {
                    -24.0
                } else if rib < 4.0 {
                    9.0
                } else {
                    0.0
                };
            }
            if (x - 90.0).abs() < 1.5 || (y - 500.0).abs() < 1.5 {
                c -= 24.0;
            }
            let well = rounded(x, y, (100.0, 40.0, 316.0, 404.0), [5.0; 4]);
            if well.abs() < 2.0 {
                c -= 35.0;
            }
            c *= 1.0 + (body + 4.0).clamp(0.0, 4.0) * -0.10;
            let connector = (160.0..320.0).contains(&x) && y > 546.0;
            let alpha = if connector {
                0.0
            } else {
                (0.5 - body).clamp(0.0, 1.0) * (0.5 + window).clamp(0.0, 1.0)
            };
            let i = (py * w + px) * 4;
            for (k, tint) in [0.99, 1.0, 1.02].into_iter().enumerate() {
                img.px[i + k] = (c * tint).clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (alpha * 255.0) as u8;
        }
    }
    Frame {
        img,
        sticker: (103, 43, 310, 398),
    }
}

/// The Lynx cart, measured from a real one: dark grey-green plastic with rounded top corners, a
/// plain band across the top (a bright bevel along its edge), the label nearly the full width
/// below it, and the edge connector's gold contacts showing along the bottom above a lip.
fn lynx_frame() -> Frame {
    use lynx::*;
    let (w, h) = LYNX_CART;
    let (fw, fh) = (w as f32, h as f32);
    let frac =
        |(x0, y0, x1, y1): (f32, f32, f32, f32)| (x0 * fw, y0 * fh, (x1 - x0) * fw, (y1 - y0) * fh);
    let (lx, ly, lw, lh) = frac(LABEL);
    let (cx, cy, cw, ch) = frac(CONTACTS);
    let lip = LIP * fh;
    let pitch = PITCH * fw;
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
        let t = t.clamp(0.0, 1.0);
        [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
    };
    let mut img = Img::new(w, h);
    for py in 0..h {
        for px in 0..w {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let body = rounded(x, y, (1.0, 1.0, fw - 2.0, fh - 2.0), RADII.map(|r| r * fw));
            let window = rounded(x, y, (lx, ly, lw, lh), [3.0; 4]);
            let (ty, tx) = (y / fh, x / fw);
            let mut c = if y >= lip {
                // The lip under the contacts, a lighter grey, with a bright edge where it starts.
                let edge = (1.0 - (y - lip).abs() / 2.5).max(0.0);
                mix(LIP_PLASTIC, [190.0, 192.0, 182.0], edge)
            } else if (cx..cx + cw).contains(&x) && (cy..cy + ch).contains(&y) {
                // Gold contacts with dark gaps between them, lit a little more on the right.
                let gold = mix(GOLD_DARK, GOLD_LIGHT, (x - cx) / cw);
                let along = ((x - cx) / pitch).fract();
                if along > CONTACT {
                    [42.0, 36.0, 26.0]
                } else {
                    gold
                }
            } else {
                // The plastic, a touch lighter at the top, with its bevel near the top edge.
                let bevel = (-((ty - BEVEL.0) / BEVEL.1).powi(2)).exp() * 32.0;
                let k = 1.0 + 0.06 * (1.0 - ty);
                PLASTIC.map(|v| v * k + bevel)
            };
            // Darker toward the outline, so the plastic reads as rounded.
            let shade = 1.0 - (body + 4.0).clamp(0.0, 4.0) * 0.08 - 0.04 * (tx - 0.5).abs();
            c = c.map(|v| v * shade);
            let i = (py * w + px) * 4;
            for (k, v) in c.into_iter().enumerate() {
                img.px[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] =
                ((0.5 - body).clamp(0.0, 1.0) * (0.5 + window).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    let sticker = (
        lx as i32 - 1,
        ly as i32 - 1,
        lw.ceil() as i32 + 2,
        lh.ceil() as i32 + 2,
    );
    Frame { img, sticker }
}

/// A Lynx cart's parts as fractions of its width and height, measured from a real one
/// (289 x 301 pixels in the photo): (left, top, right, bottom).
mod lynx {
    /// The label, nearly the cart's full width, below the plain band at the top.
    pub const LABEL: (f32, f32, f32, f32) = (0.017, 0.222, 0.993, 0.867);
    /// The art on a label, inside its black frame, as fractions of the label.
    pub const ART: (f32, f32, f32, f32) = (0.015, 0.22, 0.985, 0.83);
    /// The contacts showing along the bottom, and the lip under them (its top).
    pub const CONTACTS: (f32, f32, f32, f32) = (0.014, 0.867, 0.983, 0.944);
    pub const LIP: f32 = 0.946;
    /// A contact and the gap after it (8 of the photo's pixels), and how much of that is gold.
    pub const PITCH: f32 = 0.0277;
    pub const CONTACT: f32 = 0.75;
    /// The corners: top left, top right, bottom right, bottom left.
    pub const RADII: [f32; 4] = [0.028, 0.028, 0.012, 0.012];
    /// The bright bevel along the top: where, and how wide.
    pub const BEVEL: (f32, f32) = (0.047, 0.014);
    pub const PLASTIC: [f32; 3] = [52.0, 63.0, 67.0];
    pub const LIP_PLASTIC: [f32; 3] = [69.0, 83.0, 85.0];
    pub const GOLD_DARK: [f32; 3] = [150.0, 124.0, 92.0];
    pub const GOLD_LIGHT: [f32; 3] = [206.0, 174.0, 126.0];
}

/// The GBA cartridge's plastic, drawn the same way as the DS card's.
fn gba_frame() -> Frame {
    use gba::*;
    let (fx, fy) = (GBA_CART_W as f32 / GRID.0, GBA_CART_H as f32 / GRID.1);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    let cover = |d: f32| (0.5 - d * fx).clamp(0.0, 1.0);
    let line = |d: f32, half: f32| (1.0 - d.abs() / half).clamp(0.0, 1.0);
    let mid = GRID.0 / 2.0;
    let mut img = Img::new(GBA_CART_W, GBA_CART_H);
    for py in 0..GBA_CART_H {
        for px in 0..GBA_CART_W {
            let (x, y) = ((px as f32 + 0.5) / fx, (py as f32 + 0.5) / fy);
            let outline = rounded(x, y, GRIP, GRIP_CORNERS).min(rounded(x, y, BODY, BODY_CORNERS));

            // Lit from the top left: a shade darker toward the right and the bottom.
            let mut c = FACE * (1.04 - 0.06 * x / GRID.0 - 0.04 * y / GRID.1);

            // The grip's face, above a crease that runs across and turns up at each end.
            let mx = mid - (x - mid).abs();
            let [(tx, ty), (ex, ey)] = FACE_TURN;
            let edge = if mx >= tx {
                FACE_EDGE
            } else {
                ey + (ty - ey) * ((mx - ex) / (tx - ex)).clamp(0.0, 1.0)
            };
            if y < edge {
                c = GRIP_FACE * (1.04 - 0.06 * x / GRID.0) * mix(1.03, 0.97, y / edge);
            }
            c = mix(c, FACE * 0.8, line(y - edge, 3.0));

            // The well: a lit lip around it, a dark rim just inside, the floor under the label.
            let well = rounded(x, y, WELL, [WELL_RADIUS; 4]);
            c += 8.0 * line(well - 4.0, 4.0);
            let inside = mix(WELL_FACE, WELL_RIM, line(well + 4.0, 6.0));
            c = mix(c, inside, cover(well));

            // The triangle: pressed in, so shaded under its top edge and lit along its lower ones.
            let [a, p, b] = VEE;
            let side = |q: (f32, f32), r: (f32, f32)| {
                ((r.1 - q.1) * (x - q.0) - (r.0 - q.0) * (y - q.1)) / (r.0 - q.0).hypot(r.1 - q.1)
            };
            let tri = (a.1 - y).max(side(p, a)).max(side(b, p));
            c = mix(c, VEE_FACE, cover(tri));
            c = mix(c, FACE * 0.55, line(y - a.1 - 2.0, 3.0) * cover(tri));
            let lower = segment(x, y, a, p).min(segment(x, y, p, b));
            c += 16.0 * line(lower - 2.5, 2.5);

            // The step where the body meets the grip catches the light on its underside.
            if y > GRIP.1 + GRIP.3 - 4.0
                && y < GRIP.1 + GRIP.3 + 6.0
                && (x < BODY.0 || x > BODY.0 + BODY.2)
            {
                c *= 0.7;
            }

            // The outer edge falls away into shadow, except along the left and the top where it is lit.
            let rim = -outline;
            let lit = x < mid && (x < BODY.0 + 12.0 || y < 12.0);
            c *= if lit {
                mix(1.25, 1.0, (rim - 3.0) / 6.0)
            } else {
                mix(0.45, 1.0, (rim + 0.5) / 7.0)
            };

            let grain = (px as u32)
                .wrapping_mul(0x9E3779B1)
                .wrapping_add((py as u32).wrapping_mul(0x85EBCA6B));
            let grain = ((grain ^ (grain >> 15)).wrapping_mul(0x2C1B3C6D) >> 28) as f32 - 7.5;
            c *= 1.0 + grain * 0.004;

            let window = rounded(x, y, WINDOW, [WINDOW_RADIUS; 4]);
            let alpha = cover(outline) * (1.0 - cover(window));
            let i = (py * GBA_CART_W + px) * 4;
            // Charcoal with a cool, slightly green cast, as on the real plastic.
            for (k, tint) in [0.94, 1.03, 1.07].into_iter().enumerate() {
                img.px[i + k] = (c * tint).clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (alpha * 255.0) as u8;
        }
    }
    let (x, y, w, h) = WINDOW;
    let sticker = (
        (x * fx) as i32 - 1,
        (y * fy) as i32 - 1,
        (w * fx).ceil() as i32 + 2,
        (h * fy).ceil() as i32 + 2,
    );
    Frame { img, sticker }
}

/// The Game Boy cart's plastic, grey, drawn the same way as the GBA cartridge's.
fn gb_frame() -> Frame {
    use gb::*;
    let (fx, fy) = (GB_CART_W as f32 / GRID.0, GB_CART_H as f32 / GRID.1);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    let cover = |d: f32| (0.5 - d * fx).clamp(0.0, 1.0);
    let line = |d: f32, half: f32| (1.0 - d.abs() / half).clamp(0.0, 1.0);
    let mut img = Img::new(GB_CART_W, GB_CART_H);
    for py in 0..GB_CART_H {
        for px in 0..GB_CART_W {
            let (x, y) = ((px as f32 + 0.5) / fx, (py as f32 + 0.5) / fy);
            // The body, less the notch at the top right.
            let notch = rounded(
                x,
                y,
                (NOTCH.0, -20.0, GRID.0 - NOTCH.0 + 20.0, NOTCH.1 + 20.0),
                [0.0, 0.0, 0.0, 4.0],
            );
            let outline = rounded(x, y, BODY, CORNERS).max(-notch);

            // Lit from the top left: a shade darker toward the right and the bottom.
            let mut c = FACE * (1.03 - 0.05 * x / GRID.0 - 0.04 * y / GRID.1);

            // The ribs: each one's top catches the light, the groove under it is in shade.
            if x < RIBS.0 || x > RIBS.1 {
                for g in GROOVES {
                    c = mix(c, FACE * 1.1, line(y - (g - 9.0), 3.0) * 0.7);
                    c = mix(c, FACE * 0.62, line(y - g, 2.6));
                }
            }

            // Pressed in: the plate, the well and the triangle. Lit from above, each one's
            // upper wall is in shade and its lower wall catches the light.
            let sunk = |c: f32, d: f32, floor: f32, top: f32, bottom: f32, y: f32| {
                let wall = line(d + 2.5, 3.0);
                let lit = if y < bottom - (bottom - top) / 2.0 {
                    0.72
                } else {
                    1.12
                };
                mix(mix(c, floor, cover(d)), c * lit, wall)
            };
            let plate = rounded(x, y, PLATE, [PLATE_RADIUS; 4]);
            let (pt, pb) = (PLATE.1, PLATE.1 + PLATE.3);
            c = sunk(
                c,
                plate,
                PLATE_FACE * mix(0.94, 1.04, (y - pt) / (pb - pt)),
                pt,
                pb,
                y,
            );
            let well = rounded(x, y, WELL, [WELL_RADIUS; 4]);
            c = sunk(c, well, WELL_FACE, WELL.1, WELL.1 + WELL.3, y);
            c = mix(c, WELL_RIM, line(well + 2.0, 1.5) * cover(well));

            let [a, p, b] = VEE;
            let side = |q: (f32, f32), r: (f32, f32)| {
                ((r.1 - q.1) * (x - q.0) - (r.0 - q.0) * (y - q.1)) / (r.0 - q.0).hypot(r.1 - q.1)
            };
            let tri = (a.1 - y).max(side(p, a)).max(side(b, p));
            c = mix(c, VEE_FACE, cover(tri));
            c = mix(c, FACE * 0.7, line(y - a.1 - 1.5, 2.0) * cover(tri));
            let lower = segment(x, y, a, p).min(segment(x, y, p, b));
            c = mix(c, FACE * 1.12, line(lower - 1.5, 1.8));

            // The outer edge falls away into shadow, except along the left and the top.
            let rim = -outline;
            let lit = x < GRID.0 / 2.0 && (x < 8.0 || y < 8.0);
            c *= if lit {
                mix(1.08, 1.0, (rim - 2.0) / 5.0)
            } else {
                mix(0.6, 1.0, (rim + 0.5) / 6.0)
            };

            let grain = (px as u32)
                .wrapping_mul(0x9E3779B1)
                .wrapping_add((py as u32).wrapping_mul(0x85EBCA6B));
            let grain = ((grain ^ (grain >> 15)).wrapping_mul(0x2C1B3C6D) >> 28) as f32 - 7.5;
            c *= 1.0 + grain * 0.003;

            let window = rounded(x, y, WINDOW, [WINDOW_RADIUS; 4]);
            let alpha = cover(outline) * (1.0 - cover(window));
            let i = (py * GB_CART_W + px) * 4;
            // The DMG's warm grey.
            for (k, tint) in [1.02, 0.98, 0.97].into_iter().enumerate() {
                img.px[i + k] = (c * tint).clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (alpha * 255.0) as u8;
        }
    }
    let (x, y, w, h) = WINDOW;
    let sticker = (
        (x * fx) as i32 - 1,
        (y * fy) as i32 - 1,
        (w * fx).ceil() as i32 + 2,
        (h * fy).ceil() as i32 + 2,
    );
    Frame { img, sticker }
}

/// The Game Boy Color cart's clear plastic, and the board seen through it.
fn gbc_frame() -> Frame {
    gbc_shell(true)
}

/// The Color-only cart's shape: in its own clear plastic, or opaque (for a colour and finish of
/// a `cart_shell.txt`'s, finish.rs).
fn gbc_shell(clear: bool) -> Frame {
    let see = if clear { CLEAR } else { 0.0 };
    use gbc::*;
    let grid = gb::GRID;
    let (fx, fy) = (GB_CART_W as f32 / grid.0, GB_CART_H as f32 / grid.1);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0.0, 1.0);
    let cover = |d: f32| (0.5 - d * fx).clamp(0.0, 1.0);
    let line = |d: f32, half: f32| (1.0 - d.abs() / half).clamp(0.0, 1.0);
    let body = (0.0, 0.0, grid.0, grid.1);
    let mut img = Img::new(GB_CART_W, GB_CART_H);
    for py in 0..GB_CART_H {
        for px in 0..GB_CART_W {
            let (x, y) = ((px as f32 + 0.5) / fx, (py as f32 + 0.5) / fy);
            let outline = rounded(x, y, body, CORNERS);

            // What is behind the plastic: the board, its chips and its contacts, or nothing.
            let board = cover(rounded(x, y, BOARD, [6.0; 4]));
            let mut behind = BOARD_COLOUR;
            for chip in CHIPS {
                let on = cover(rounded(x, y, chip, [2.0; 4]));
                behind = std::array::from_fn(|k| mix(behind[k], CHIP_COLOUR[k], on));
            }
            let (cx, cy, cw, ch) = CONTACTS;
            let pitch = cw / PINS as f32;
            let pin = ((x - cx) / pitch).fract() < 0.62 && x > cx && x < cx + cw;
            let gold = cover(rounded(x, y, (cx, cy, cw, ch), [1.0; 4])) * f32::from(u8::from(pin));
            behind = std::array::from_fn(|k| mix(behind[k], GOLD[k], gold));

            // The plastic: tinted, lighter where its edges and the well's catch the light.
            let mut shine = 0.0;
            let rim = -outline;
            shine += 0.35 * line(rim - 3.0, 4.0) + 0.2 * line(rim - 14.0, 3.0);
            let well = rounded(x, y, WELL, [WELL_RADIUS; 4]);
            shine += 0.3 * line(well - 1.5, 2.0);
            let (r0, r1, n) = RIBS;
            if (x < 14.0 || x > grid.0 - 14.0) && y > r0 && y < r1 {
                let at = (y - r0) / ((r1 - r0) / n);
                shine += 0.3 * line(at.fract() - 0.5, 0.25);
            }
            // Lit from the top left.
            let light = 1.04 - 0.08 * x / grid.0 - 0.06 * y / grid.1 + shine;
            let plastic = PLASTIC.map(|v| v * light);
            let colour: [f32; 3] =
                std::array::from_fn(|k| mix(plastic[k], behind[k] * 0.9, see * board));
            // Clear plastic over nothing lets the shelf show through.
            let solid = if clear {
                mix(0.62 + shine, 1.0, board)
            } else {
                1.0
            };

            let window = rounded(x, y, WINDOW, [WINDOW_RADIUS; 4]);
            let alpha = cover(outline) * (1.0 - cover(window)) * solid.min(1.0);
            let i = (py * GB_CART_W + px) * 4;
            for (k, v) in colour.into_iter().enumerate() {
                img.px[i + k] = v.clamp(0.0, 255.0) as u8;
            }
            img.px[i + 3] = (alpha * 255.0) as u8;
        }
    }
    let (x, y, w, h) = WINDOW;
    let sticker = (
        (x * fx) as i32 - 1,
        (y * fy) as i32 - 1,
        (w * fx).ceil() as i32 + 2,
        (h * fy).ceil() as i32 + 2,
    );
    Frame { img, sticker }
}

/// Where a game's box art is: its own (`Imgs/<rom>.png`, an app's icon) where there is one;
/// else what the game page fetched for it into DSlot's `cache` (details.rs), where it did; else
/// its own place, where nothing is, and the box is printed.
pub fn box_art(game: &Game, cache: &Path) -> std::path::PathBuf {
    let own = game.art("Imgs");
    if own.is_file() {
        return own;
    }
    let fetched = crate::details::boxart_file(cache, game);
    if fetched.is_file() {
        fetched
    } else {
        own
    }
}

/// A game's cart and its box, the box art read once for both.
pub fn art(shell: &Shell, game: &Game, fonts: &Fonts, looks: Looks, cache: &Path) -> (Img, Img) {
    let src = load_png(&box_art(game, cache));
    // An app's page is a grid of boxes alone: no cart to draw.
    let cart = if disc::is_disc(game.console) {
        disc::disc(
            game,
            fonts,
            load_png(&game.art("Labels")).as_ref(),
            src.as_ref(),
        )
    } else if has_shell(game.console) {
        cart(shell, game, fonts, src.as_ref(), looks)
    } else {
        Img::new(1, 1)
    };
    (cart, boxart(game, fonts, looks.black(game), src.as_ref()))
}

/// `Carts/<rom>.png` is a finished cart and is drawn as it is. `Labels/<rom>.png` is the
/// sticker alone and goes behind the frame. With neither, a sticker is printed from the ROM.
/// A GBA cart with no label of its own is printed from its box art.
fn cart(shell: &Shell, game: &Game, fonts: &Fonts, cover: Option<&Img>, looks: Looks) -> Img {
    let (w, h) = cart_size(game.console);
    let mut img = Img::new(w, h);
    let full = |src: &Img| (0.0, 0.0, src.w as f32, src.h as f32);
    if let Some(c) = load_png(&game.art("Carts")) {
        img.blit(&c, full(&c), (0, 0, w as i32, h as i32), false);
        return img;
    }
    // A SNES cart is one of two shapes, its label laid out to match.
    let us = game.console == Console::Snes && looks.us_snes(game);
    // The shell in a colour and finish of Cart Studio's (`cart_shell.txt`), else as the
    // system's carts are.
    let cards = studio::cards(game);
    let styled = studio::style(game, &cards).map(|s| shell.styled(game.console, s));
    let frame = styled
        .as_deref()
        .unwrap_or_else(|| shell.cart_frame(game.console, us));
    let (sw, sh) = (frame.sticker.2, frame.sticker.3);
    // DSlot's own label, else Cart Studio's: any size, scaled to fill and cropped to fit.
    let label = studio::labels(game, &cards)
        .into_iter()
        .find_map(|(path, studio)| Some((load_png(&path)?, studio)));
    let label = label.map(|(img, studio)| match studio {
        true => cover_label(Some(&img), sw, sh),
        false => img,
    });
    let sticker = label.unwrap_or_else(|| match game.console {
        Console::Ds => printed(game, fonts, sw, sh),
        // A Lynx label frames its art in black: a deep band at the top, a strip at the bottom.
        Console::Lynx => lynx_label(game, fonts, sw, sh, cover),
        // As each system's labels lay their art out.
        Console::MegaDrive => shells::md_label(game, fonts, sw, sh, cover),
        Console::GameGear => shells::gg_label(game, fonts, sw, sh, cover),
        Console::Snes if us => shells::us_label(game, fonts, sw, sh, cover),
        Console::Snes => shells::famicom_label(game, fonts, sw, sh, cover),
        // A Game Boy label is about square, as its box is: the box art fills it.
        Console::Gb | Console::Gbc | Console::Nes if cover.is_some() => cover_label(cover, sw, sh),
        Console::Gba
        | Console::Gb
        | Console::Gbc
        | Console::Nes
        | Console::GameWatch
        | Console::Apps
        | Console::Ports
        | Console::Fds
        | Console::MasterSystem
        | Console::Sg1000
        | Console::PcEngine
        | Console::PcEngineCd
        | Console::PlayStation
        | Console::NeoGeoPocket
        | Console::WonderSwan => printed_gba(game, fonts, sw, sh, cover),
    });
    img.blit(&sticker, full(&sticker), frame.sticker, false);
    img.blit(
        &frame.img,
        full(&frame.img),
        (0, 0, w as i32, h as i32),
        false,
    );
    img
}

/// A Lynx label made from the box art (or printed without it): the art inside a black frame,
/// as on every Lynx cart, a deep band across the top and a strip along the bottom.
fn lynx_label(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    let (l, t, r, b) = lynx::ART;
    let (ax, ay) = ((l * w as f32) as i32, (t * h as f32) as i32);
    let (aw, ah) = (((r - l) * w as f32) as i32, ((b - t) * h as f32) as i32);
    let art = match cover {
        Some(_) => cover_label(cover, aw, ah),
        None => printed_gba(game, fonts, aw, ah, None),
    };
    let mut img = Img::new(w as usize, h as usize);
    img.rrect(0.0, 0.0, w as f32, h as f32, [3.0; 4], [14, 14, 16, 255]);
    img.blit(
        &art,
        (0.0, 0.0, art.w as f32, art.h as f32),
        (ax, ay, aw, ah),
        false,
    );
    img
}

/// A label that is the box art, filling it and cropped about its centre.
fn cover_label(cover: Option<&Img>, w: i32, h: i32) -> Img {
    let mut img = Img::new(w as usize, h as usize);
    if let Some(src) = cover {
        let k = (src.w as f32 / w as f32).min(src.h as f32 / h as f32);
        let (cw, ch) = (w as f32 * k, h as f32 * k);
        img.blit(
            src,
            ((src.w as f32 - cw) / 2.0, (src.h as f32 - ch) / 2.0, cw, ch),
            (0, 0, w, h),
            false,
        );
    }
    img
}

/// `src` filling `w` x `h`, cropped about its centre, blurred by going through a tiny copy and
/// dimmed: what a cover or a picture of the unit sits on when it is shown whole.
fn backdrop(src: &Img, w: usize, h: usize) -> Img {
    let mut img = Img::new(w, h);
    let k = (src.w as f32 / w as f32).min(src.h as f32 / h as f32);
    let (cw, ch) = (w as f32 * k, h as f32 * k);
    let tiny = {
        let mut t = Img::new(w / 12 + 1, h / 12 + 1);
        t.blit(
            src,
            ((src.w as f32 - cw) / 2.0, (src.h as f32 - ch) / 2.0, cw, ch),
            (0, 0, t.w as i32, t.h as i32),
            false,
        );
        t
    };
    // Scaled up bilinearly, so it reads as a blur rather than as blocks, and dimmed.
    for y in 0..h {
        for x in 0..w {
            let fx =
                ((x as f32 + 0.5) * tiny.w as f32 / w as f32 - 0.5).clamp(0.0, (tiny.w - 1) as f32);
            let fy =
                ((y as f32 + 0.5) * tiny.h as f32 / h as f32 - 0.5).clamp(0.0, (tiny.h - 1) as f32);
            let (x0, y0) = (fx as usize, fy as usize);
            let (x1, y1) = ((x0 + 1).min(tiny.w - 1), (y0 + 1).min(tiny.h - 1));
            let (tx, ty) = (fx.fract(), fy.fract());
            let i = (y * w + x) * 4;
            for k in 0..3 {
                let at = |px: usize, py: usize| tiny.px[(py * tiny.w + px) * 4 + k] as f32;
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                img.px[i + k] = ((top + (bottom - top) * ty) * 0.45) as u8;
            }
            img.px[i + 3] = 255;
        }
    }
    img
}

/// A GBA label made from the box art: the cover whole on the left over a dimmed, enlarged copy
/// of itself, the name and maker beside it and the game code in the corner.
fn printed_gba(game: &Game, fonts: &Fonts, w: i32, h: i32, cover: Option<&Img>) -> Img {
    let mut img = Img::new(w as usize, h as usize);
    let [r, g, b] = name_colour(game);
    img.rrect(
        0.0,
        0.0,
        w as f32,
        h as f32,
        [w as f32 * 0.015; 4],
        [r, g, b, 255],
    );
    let pad = (h as f32 * 0.07) as i32;
    let side = h - 2 * pad;
    if let Some(src) = cover {
        // Behind: the cover filling the label, blurred and dimmed.
        img.px = backdrop(src, w as usize, h as usize).px;
        let s = src.w.min(src.h) as f32;
        img.blit(
            src,
            ((src.w as f32 - s) / 2.0, (src.h as f32 - s) / 2.0, s, s),
            (pad, pad, side, side),
            false,
        );
    }
    let x = if cover.is_some() {
        pad * 2 + side
    } else {
        pad * 2
    };
    let room = (w - x - pad * 2) as f32;
    let size = fonts.fit(game.name(), h as f32 * 0.16, 14.0, room, true);
    let t = fonts.render(game.name(), size, true);
    let mut y = (h as f32 * 0.3) as i32;
    img.stamp(&t, x, y, [255, 255, 255, 255]);
    y += (size * 1.35) as i32;
    if let Some(maker) = game.title.get(1) {
        let t = fonts.render(maker, h as f32 * 0.08, false);
        img.stamp(&t, x, y, [255, 255, 255, 200]);
    }
    let t = fonts.render(&game.serial(), h as f32 * 0.065, true);
    img.stamp(
        &t,
        w - t.w as i32 - pad,
        h - t.h as i32 - pad,
        [255, 255, 255, 220],
    );
    img
}

/// A colour picked from the name, for a game with no icon.
fn name_colour(game: &Game) -> [u8; 3] {
    if let Some(icon) = &game.icon {
        return icon_colour(icon);
    }
    let hash = game.name().bytes().fold(0x811c9dc5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x01000193)
    });
    let hue = (hash % 360) as f32;
    let f = |n: f32| {
        let k = (n + hue / 60.0) % 6.0;
        (110.0 - 70.0 * (k.min(4.0 - k).clamp(0.0, 1.0))) as u8
    };
    [f(5.0), f(3.0), f(1.0)]
}

/// A sticker made from what the ROM carries: title on the band, icon as the artwork, game code below.
fn printed(game: &Game, fonts: &Fonts, w: i32, h: i32) -> Img {
    const INK: Rgba = [48, 50, 56, 255];
    let mut img = Img::new(w as usize, h as usize);
    img.rrect(
        0.0,
        0.0,
        w as f32,
        h as f32,
        [w as f32 * 0.05; 4],
        [243, 242, 238, 255],
    );
    let (band, strip) = ((h as f32 * 0.18) as i32, (h as f32 * 0.15) as i32);
    let art_h = h - band - strip;

    let size = fonts.fit(game.name(), band as f32 * 0.52, 14.0, w as f32 - 36.0, true);
    let t = fonts.render(game.name(), size, true);
    img.stamp(&t, (w - t.w as i32) / 2, (band - t.h as i32) / 2 + 2, INK);

    let [r, g, b] = game.icon.as_ref().map(icon_colour).unwrap_or([70, 80, 100]);
    img.rrect(
        0.0,
        band as f32,
        w as f32,
        art_h as f32,
        [0.0; 4],
        [r / 3 + 20, g / 3 + 22, b / 3 + 28, 255],
    );
    if let Some(icon) = &game.icon {
        let k = ((w.min(art_h) as f32 * 0.82) as i32 / 32).max(1) * 32;
        img.blit(
            icon,
            (0.0, 0.0, 32.0, 32.0),
            ((w - k) / 2, band + (art_h - k) / 2, k, k),
            true,
        );
    }

    let t = fonts.render(&game.serial(), h as f32 * 0.055, true);
    img.stamp(
        &t,
        w - t.w as i32 - 16,
        h - strip + (strip - t.h as i32) / 2 - 2,
        INK,
    );
    img
}

/// Box front: the cover in a DS case, the plastic showing as a rounded frame around it.
/// Every box is the same shape, as on a real shelf.
/// A GBA (GB, GBC) box is printed card: the cover to its edges, with a pale rim where the card folds.
fn boxart(game: &Game, fonts: &Fonts, black: bool, src: Option<&Img>) -> Img {
    // An app's box is its icon, as it is.
    if game.console == Console::Apps {
        let (w, h) = GBA_BOX;
        return cover(game, fonts, src, w, h);
    }
    if game.console != Console::Ds {
        let (w, h) = box_size(game.console);
        let cover = cover(game, fonts, src, w - 4, h - 4);
        let mut img = Img::new(w, h);
        img.rrect(0.0, 0.0, w as f32, h as f32, [4.0; 4], [226, 226, 222, 255]);
        img.blit(
            &cover,
            (0.0, 0.0, cover.w as f32, cover.h as f32),
            (2, 2, cover.w as i32, cover.h as i32),
            false,
        );
        return img;
    }
    let ((w, h), e) = (CASE_SIZE, CASE_EDGE as i32);
    let cover = cover(game, fonts, src, w - 2 * CASE_EDGE, h - 2 * CASE_EDGE);
    let [plastic, rim] = if black { CASE_BLACK } else { CASE_CLEAR };
    let mut img = Img::new(w, h);
    // A lighter rim a pixel wide catches the light on the case's edge.
    img.rrect(0.0, 0.0, w as f32, h as f32, [CASE_RADIUS; 4], rim);
    img.rrect(
        1.0,
        1.0,
        w as f32 - 2.0,
        h as f32 - 2.0,
        [CASE_RADIUS - 1.0; 4],
        plastic,
    );
    img.blit(
        &cover,
        (0.0, 0.0, cover.w as f32, cover.h as f32),
        (e, e, cover.w as i32, cover.h as i32),
        false,
    );
    img
}

/// Fit a box within a grid cell, preserving landscape covers as well as square ones.
pub fn thumb_size(console: Console, size: (usize, usize)) -> (usize, usize) {
    if console == Console::Ds {
        return size; // Keep the DS grid and its existing cached thumbnails unchanged.
    }
    let (bw, bh) = box_size(console);
    (size.0.min(size.1 * bw / bh), size.1.min(size.0 * bh / bw))
}

/// The box at grid size. Reading and scaling a cover is the slow part of showing it, so the
/// result is kept in `cache` and reused until the cover (or, without one, the ROM) changes.
/// A box is drawn as high as a DS one, at its own shape: a GBA box is square.
pub fn thumb(game: &Game, fonts: &Fonts, black: bool, size: (usize, usize), cache: &Path) -> Img {
    let (w, h) = thumb_size(game.console, size);
    let art = box_art(game, cache);
    let source = fs::metadata(&art)
        .or_else(|_| fs::metadata(&game.path))
        .ok();
    let stamp = source.map_or([0; 2], |m| {
        [
            m.len(),
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs()),
        ]
    });
    let head: Vec<u8> = stamp.iter().flat_map(|v| v.to_le_bytes()).collect();

    // FNV-1a over everything the picture depends on bar the source's contents.
    let key = format!("{}|{black}|{w}x{h}", game.path.display());
    let hash = key.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    let file = cache.join(format!("{hash:016x}.thumb"));
    if let Ok(bytes) = fs::read(&file) {
        if bytes.len() == head.len() + w * h * 4 && bytes.starts_with(&head) {
            return Img {
                w,
                h,
                px: bytes[head.len()..].to_vec(),
            };
        }
    }

    let src = load_png(&art);
    let img = boxart(game, fonts, black, src.as_ref()).shrink(w, h);
    let _ = fs::create_dir_all(cache);
    let _ = fs::write(&file, [head.as_slice(), &img.px].concat());
    img
}

/// `Imgs/<rom>.png` scaled to fill `w` x `h` and cropped about its centre, or a generated cover.
fn cover(game: &Game, fonts: &Fonts, src: Option<&Img>, w: usize, h: usize) -> Img {
    if let Some(src) = src {
        // A Game & Watch picture is the unit itself, of whatever shape: shown whole, over itself.
        if game.console == Console::GameWatch {
            let mut img = backdrop(src, w, h);
            let k = (w as f32 / src.w as f32).min(h as f32 / src.h as f32);
            let (dw, dh) = ((src.w as f32 * k) as i32, (src.h as f32 * k) as i32);
            img.blit(
                src,
                (0.0, 0.0, src.w as f32, src.h as f32),
                ((w as i32 - dw) / 2, (h as i32 - dh) / 2, dw, dh),
                false,
            );
            return img;
        }
        let k = (src.w as f32 / w as f32).min(src.h as f32 / h as f32);
        let (sw, sh) = (w as f32 * k, h as f32 * k);
        let mut img = Img::new(w, h);
        img.blit(
            src,
            ((src.w as f32 - sw) / 2.0, (src.h as f32 - sh) / 2.0, sw, sh),
            (0, 0, w as i32, h as i32),
            false,
        );
        return img;
    }

    let strip = 48;
    let [r, g, b] = name_colour(game);
    let mut img = Img::new(w, h);
    for y in 0..h {
        // Lighter at the top, darker at the bottom.
        let k = 0.95 - 0.55 * y as f32 / h as f32;
        let c = [
            (r as f32 * k) as u8 + 12,
            (g as f32 * k) as u8 + 12,
            (b as f32 * k) as u8 + 16,
            255,
        ];
        for x in strip..w {
            img.px[(y * w + x) * 4..][..4].copy_from_slice(&c);
        }
        for x in 0..strip {
            img.px[(y * w + x) * 4..][..4].copy_from_slice(&[22, 24, 29, 255]);
        }
    }
    let panel = (w - strip) as i32;
    if let Some(icon) = &game.icon {
        img.blit(
            icon,
            (0.0, 0.0, 32.0, 32.0),
            (strip as i32 + (panel - 224) / 2, 44, 224, 224),
            true,
        );
    }
    let size = fonts.fit(game.name(), 40.0, 18.0, panel as f32 - 40.0, true);
    let t = fonts.render(game.name(), size, true);
    // Landscape boxes (SNES, Game & Watch) are shorter: keep the title and publisher on them.
    let title_y = 312.min(h.saturating_sub(118)) as i32;
    img.stamp(
        &t,
        strip as i32 + (panel - t.w as i32) / 2,
        title_y,
        [255, 255, 255, 255],
    );
    if let Some(maker) = game.title.last().filter(|_| game.title.len() > 1) {
        let t = fonts.render(maker, 20.0, false);
        img.stamp(
            &t,
            strip as i32 + (panel - t.w as i32) / 2,
            title_y + 52,
            [255, 255, 255, 190],
        );
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn alpha(f: &Frame, x: usize, y: usize) -> u8 {
        f.img.px[(y * f.img.w + x) * 4 + 3]
    }

    #[test]
    fn landscape_snes_covers_fit_the_grid() {
        assert_eq!(thumb_size(Console::Snes, (180, 164)), (180, 126));
        assert_eq!(thumb_size(Console::Gba, (180, 164)), (164, 164));
        assert_eq!(thumb_size(Console::Ds, (180, 164)), (180, 164));
        assert!(has_shell(Console::Snes));
        // A Game & Watch unit has no cart, but its page is shown: as boxes, landscape.
        assert!(!has_shell(Console::GameWatch));
        let (w, h) = box_size(Console::GameWatch);
        assert!(w > h);
    }

    /// Every shell: cart-sized, its sticker's window inside it and clear, its face solid.
    #[test]
    fn shells() {
        for console in Console::ALL.into_iter().filter(|&c| shell_of(c).is_some()) {
            let f = shell_of(console).unwrap()();
            let (w, h) = cart_size(console);
            assert_eq!((f.img.w, f.img.h), (w, h), "{console:?}");
            let (sx, sy, sw, sh) = f.sticker;
            assert!(sx >= 0 && sy >= 0, "{console:?}");
            assert!(
                (sx + sw) as usize <= w && (sy + sh) as usize <= h,
                "{console:?}"
            );
            let centre = ((sx + sw / 2) as usize, (sy + sh / 2) as usize);
            assert_eq!(alpha(&f, centre.0, centre.1), 0, "{console:?} window");
            // Half way between the left edge and the label, half way down: the face (on the
            // Color-only cart, the board behind it).
            assert_eq!(alpha(&f, sx as usize / 2, h / 2), 255, "{console:?} face");
        }
    }

    /// The SNES's two shapes: both have their label window, and the Super Famicom's is the
    /// narrower, drawn on the same canvas.
    #[test]
    fn snes_shapes() {
        let (us, famicom) = (shells::snes_us_frame(), shells::snes_famicom_frame());
        for f in [&us, &famicom] {
            assert_eq!((f.img.w, f.img.h), SNES_CART);
            let (sx, sy, sw, sh) = f.sticker;
            assert!(
                sx >= 0
                    && sy >= 0
                    && (sx + sw) as usize <= f.img.w
                    && (sy + sh) as usize <= f.img.h
            );
            assert_eq!(alpha(f, (sx + sw / 2) as usize, (sy + sh / 2) as usize), 0);
            // Below the label, the grip's middle: plastic.
            assert_eq!(alpha(f, f.img.w / 2, f.img.h * 3 / 4), 255);
        }
        assert_eq!(alpha(&us, 3, SNES_CART.1 / 2), 255);
        assert_eq!(alpha(&famicom, 3, SNES_CART.1 / 2), 0);
    }

    #[test]
    fn snes_cart_shape_by_setting_or_region() {
        let game = |stem: &str| Game {
            console: Console::Snes,
            path: format!("/nowhere/{stem}.sfc").into(),
            stem: stem.into(),
            code: String::new(),
            title: vec![stem.into()],
            icon: None,
            cover: None,
            gone: false,
        };
        let looks = |snes| Looks {
            case: Case::Region,
            snes,
        };
        let (usa, japan) = (game("Star Harbor (USA)"), game("Moon Garden (Japan)"));
        assert!(looks(SnesCart::Region).us_snes(&usa));
        assert!(!looks(SnesCart::Region).us_snes(&japan));
        assert!(looks(SnesCart::Us).us_snes(&japan));
        assert!(!looks(SnesCart::Famicom).us_snes(&usa));
        assert_eq!(
            crate::rom::snes_north_american(Path::new("/x.sfc"), "Moss (Europe)"),
            Some(false)
        );
        assert_eq!(
            crate::rom::snes_north_american(Path::new("/x.sfc"), "Moss (U) [!]"),
            Some(true)
        );
        assert_eq!(
            crate::rom::snes_north_american(Path::new("/x.sfc"), "Moss"),
            None
        );
    }

    /// A tall picture of a Game & Watch unit is shown whole in its wide box, over a dimmed copy
    /// of itself, rather than cropped across the middle.
    #[test]
    fn game_watch_unit_whole() {
        let game = Game {
            console: Console::GameWatch,
            path: "/nowhere/gnw_tall.7z".into(),
            stem: "gnw_tall".into(),
            code: String::new(),
            title: vec!["Tall".into()],
            icon: None,
            cover: None,
            gone: false,
        };
        // Red at the top, blue at the bottom: a crop to the box's shape would lose both.
        let mut src = Img::new(100, 160);
        for (i, px) in src.px.chunks_mut(4).enumerate() {
            px.copy_from_slice(if i < 100 * 10 {
                &[255, 0, 0, 255]
            } else if i >= 100 * 150 {
                &[0, 0, 255, 255]
            } else {
                &[0, 255, 0, 255]
            });
        }
        let (w, h) = (480, 300);
        let img = cover(&game, &Fonts::load(), Some(&src), w, h);
        let at = |x: usize, y: usize| &img.px[(y * w + x) * 4..][..3];
        assert_eq!(at(w / 2, 2), [255, 0, 0]);
        assert_eq!(at(w / 2, h - 3), [0, 0, 255]);
        // Beside the picture, the backdrop: green, dimmed.
        assert!(at(10, h / 2)[1] < 130 && at(10, h / 2)[1] > 60);
    }

    /// A game on a card of its own, `Roms/<folder>/<stem>.<ext>`, for Cart Studio's files.
    fn studio_game(card: &Path, console: Console, stem: &str, ext: &str) -> Game {
        let dir = card.join("Roms").join(console.folder());
        fs::create_dir_all(&dir).unwrap();
        Game {
            console,
            path: dir.join(format!("{stem}.{ext}")),
            stem: stem.into(),
            code: String::new(),
            title: vec![stem.into()],
            icon: None,
            cover: None,
            gone: false,
        }
    }

    fn looks() -> Looks {
        Looks {
            case: Case::Region,
            snes: SnesCart::Region,
        }
    }

    /// Writes `img` to `$DSLOT_RENDERS/<name>.png` when that's set, to look at.
    fn keep(img: &Img, name: &str) {
        if let Some(dir) = std::env::var_os("DSLOT_RENDERS") {
            let path = Path::new(&dir).join(format!("{name}.png"));
            crate::gfx::save_png(&path, img.w as u32, img.h as u32, &img.px).unwrap();
        }
    }

    /// Cart Studio's shells: a GB cart in clear purple, a GBA in glitter, a GBC in solid teal,
    /// each drawn from a `cart_shell.txt` in the card's `Labels/`.
    #[test]
    fn cart_studio_finishes() {
        let dir = TempDir::new("cart-studio");
        let card = dir.path();
        fs::create_dir_all(card.join("Labels")).unwrap();
        fs::write(
            card.join("Labels/cart_shell.txt"),
            "Purple = notched 7b3fb0 clear\nSparkle = auto 2a5bd7 glitter\nTeal = rounded 1fa39a solid\n",
        )
        .unwrap();
        let (shell, fonts) = (Shell::load(), Fonts::load());
        let draw = |console, stem: &str, ext| {
            let game = studio_game(card, console, stem, ext);
            let img = cart(&shell, &game, &fonts, None, looks());
            keep(&img, &format!("{}-{stem}", console.folder()));
            img
        };
        let at = |img: &Img, x: usize, y: usize| {
            let i = (y * img.w + x) * 4;
            [img.px[i], img.px[i + 1], img.px[i + 2], img.px[i + 3]]
        };

        // Clear purple: the board shows through low down, the shelf through the plastic
        // beside it, and the plastic is purple (red and blue over green).
        let gb = draw(Console::Gb, "Purple", "gb");
        assert_eq!((gb.w, gb.h), (GB_CART_W, GB_CART_H));
        let edge = at(&gb, 6, GB_CART_H / 2);
        assert!(
            edge[3] < 255 && edge[3] > 100,
            "see-through at the edge: {edge:?}"
        );
        let face = at(&gb, 30, GB_CART_H / 2);
        assert!(face[0] > face[1] && face[2] > face[1], "purple: {face:?}");
        // The notch is the Game Boy's.
        assert_eq!(at(&gb, GB_CART_W - 6, 6)[3], 0);

        // Glitter: blue, flecked, with sparkles near white.
        let gba = draw(Console::Gba, "Sparkle", "gba");
        let face: Vec<[u8; 4]> = (0..GBA_CART_W)
            .flat_map(|x| (0..60).map(move |y| (x, y + 10)))
            .map(|(x, y)| at(&gba, x, y))
            .filter(|p| p[3] == 255)
            .collect();
        let sparkles = face.iter().filter(|p| p[0] > 200 && p[1] > 200).count();
        assert!(
            sparkles > face.len() / 400,
            "{sparkles} sparkles in {}",
            face.len()
        );
        let blue = face.iter().filter(|p| p[2] > p[0]).count();
        assert!(blue > face.len() / 2);

        // Solid teal, the rounded Color-only shape: opaque, green and blue over red.
        let gbc = draw(Console::Gbc, "Teal", "gbc");
        let face = at(&gbc, 30, GB_CART_H / 2);
        assert_eq!(face[3], 255);
        assert!(face[1] > face[0] && face[2] > face[0], "teal: {face:?}");
        assert!(at(&gbc, GB_CART_W - 2, 2)[3] < 255);
        assert!(at(&gbc, GB_CART_W - 12, 12)[3] == 255, "no notch");
    }

    /// Labels: DSlot's own beside the ROM first, then Cart Studio's in the card's
    /// `Labels/<system>/`, which is cropped to the sticker's shape rather than stretched.
    #[test]
    fn cart_studio_labels() {
        let dir = TempDir::new("cart-studio-labels");
        let card = dir.path();
        let game = studio_game(card, Console::Gba, "Wide", "gba");
        // Much wider than a GBA label: red at the sides, blue across the middle.
        let (w, h) = (3000usize, 661usize);
        let mut src = Img::new(w, h);
        for (i, px) in src.px.chunks_mut(4).enumerate() {
            let x = i % w;
            let red = x < w / 6 || x > w - w / 6;
            px.copy_from_slice(if red {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
        let studio = card.join("Labels/GBA/Wide.png");
        fs::create_dir_all(studio.parent().unwrap()).unwrap();
        crate::gfx::save_png(&studio, w as u32, h as u32, &src.px).unwrap();
        let (shell, fonts) = (Shell::load(), Fonts::load());
        let img = cart(&shell, &game, &fonts, None, looks());
        let (sx, sy, _, sh) = shell.frame(Console::Gba).sticker;
        let at = |x: i32, y: i32| {
            let i = (y as usize * img.w + x as usize) * 4;
            [img.px[i], img.px[i + 1], img.px[i + 2]]
        };
        // Cropped from the centre: the sides are gone, blue to the window's edge.
        assert_eq!(at(sx + 8, sy + sh / 2), [0, 0, 255]);

        // DSlot's own label wins.
        let own = game.art("Labels");
        fs::create_dir_all(own.parent().unwrap()).unwrap();
        let green = Img {
            w: 8,
            h: 8,
            px: [0, 255, 0, 255].repeat(64),
        };
        crate::gfx::save_png(&own, 8, 8, &green.px).unwrap();
        let img = cart(&shell, &game, &fonts, None, looks());
        let i = ((sy + sh / 2) as usize * img.w + (sx + 8) as usize) * 4;
        assert_eq!(&img.px[i..i + 3], [0, 255, 0]);
    }

    /// The Game Boy's notch is cut out of the top right corner; the Color-only cart has none.
    #[test]
    fn game_boy_notch() {
        let (w, h) = (GB_CART_W, GB_CART_H);
        let gb = gb_frame();
        assert_eq!(alpha(&gb, w - 6, 6), 0);
        assert_eq!(alpha(&gb, w - 6, h / 4), 255);
        assert_eq!(alpha(&gb, w / 2, 6), 255);
        let gbc = gbc_frame();
        assert!(alpha(&gbc, w - 6, 8) > 0);
    }
}
