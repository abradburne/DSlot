//! Reads the touch panel straight from evdev, the way the stock DS emulator does.
//! SDL's own touch/mouse events are used everywhere else (desktop preview).

#[derive(Clone, Copy, Debug)]
pub enum Pointer {
    Down(f32, f32),
    Move(f32, f32),
    Up,
}

#[cfg(target_os = "linux")]
pub use linux::Panel;

#[cfg(not(target_os = "linux"))]
pub struct Panel;

#[cfg(not(target_os = "linux"))]
impl Panel {
    pub fn open() -> Option<Panel> {
        None
    }
    pub fn poll(&mut self, _out: &mut Vec<Pointer>) {}
}

#[cfg(target_os = "linux")]
mod linux {
    use super::Pointer;
    use std::ffi::CString;
    use std::os::raw::{c_int, c_ulong};

    const EV_SYN: u16 = 0;
    const EV_KEY: u16 = 1;
    const EV_ABS: u16 = 3;
    const BTN_TOUCH: u16 = 0x14a;
    const ABS_X: u16 = 0x00;
    const ABS_Y: u16 = 0x01;
    const ABS_MT_POSITION_X: u16 = 0x35;
    const ABS_MT_POSITION_Y: u16 = 0x36;
    const ABS_MT_TRACKING_ID: u16 = 0x39;

    const fn ior(nr: u32, size: u32) -> c_ulong {
        ((2u32 << 30) | (size << 16) | (0x45 << 8) | nr) as c_ulong
    }

    #[repr(C)]
    #[derive(Default)]
    struct AbsInfo {
        value: i32,
        minimum: i32,
        maximum: i32,
        fuzz: i32,
        flat: i32,
        resolution: i32,
    }

    #[repr(C)]
    struct InputEvent {
        time: libc::timeval,
        kind: u16,
        code: u16,
        value: i32,
    }

    pub struct Panel {
        fd: c_int,
        x: (i32, i32),
        y: (i32, i32),
        raw: (i32, i32),
        down: bool,
        was_down: bool,
        moved: bool,
    }

    impl Panel {
        /// Opens `DSLOT_TOUCH_DEV` if set, otherwise the first Goodix panel under /dev/input.
        pub fn open() -> Option<Panel> {
            if let Ok(dev) = std::env::var("DSLOT_TOUCH_DEV") {
                return Panel::try_open(&dev, true);
            }
            (0..16).find_map(|i| Panel::try_open(&format!("/dev/input/event{i}"), false))
        }

        fn try_open(path: &str, any_name: bool) -> Option<Panel> {
            let cpath = CString::new(path).ok()?;
            let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
            if fd < 0 {
                return None;
            }
            let mut name = [0u8; 128];
            unsafe { libc::ioctl(fd, ior(0x06, 128) as _, name.as_mut_ptr()) };
            let name = String::from_utf8_lossy(&name).to_lowercase();
            let range = |code: u16| {
                let mut info = AbsInfo::default();
                let ok = unsafe { libc::ioctl(fd, ior(0x40 + code as u32, 24) as _, &mut info) } >= 0;
                (ok && info.maximum > info.minimum).then_some((info.minimum, info.maximum))
            };
            let x = range(ABS_MT_POSITION_X).or_else(|| range(ABS_X));
            let y = range(ABS_MT_POSITION_Y).or_else(|| range(ABS_Y));
            match (any_name || name.contains("gt9") || name.contains("goodix"), x, y) {
                (true, Some(x), Some(y)) => {
                    eprintln!("touch: {path} \"{}\" x {x:?} y {y:?}", name.trim_end_matches('\0'));
                    Some(Panel { fd, x, y, raw: (0, 0), down: false, was_down: false, moved: false })
                }
                _ => {
                    unsafe { libc::close(fd) };
                    None
                }
            }
        }

        /// Drain pending events. Positions are normalised to 0..1 across the panel.
        pub fn poll(&mut self, out: &mut Vec<Pointer>) {
            let mut ev = std::mem::MaybeUninit::<InputEvent>::uninit();
            let size = std::mem::size_of::<InputEvent>();
            loop {
                let n = unsafe { libc::read(self.fd, ev.as_mut_ptr().cast(), size) };
                if n != size as isize {
                    break;
                }
                let ev = unsafe { ev.assume_init_ref() };
                match (ev.kind, ev.code) {
                    (EV_ABS, ABS_MT_POSITION_X | ABS_X) => (self.raw.0, self.moved) = (ev.value, true),
                    (EV_ABS, ABS_MT_POSITION_Y | ABS_Y) => (self.raw.1, self.moved) = (ev.value, true),
                    (EV_ABS, ABS_MT_TRACKING_ID) => self.down = ev.value >= 0,
                    (EV_KEY, BTN_TOUCH) => self.down = ev.value != 0,
                    (EV_SYN, 0) => self.report(out),
                    _ => {}
                }
            }
        }

        fn report(&mut self, out: &mut Vec<Pointer>) {
            let norm = |v: i32, (lo, hi): (i32, i32)| ((v - lo) as f32 / (hi - lo) as f32).clamp(0.0, 1.0);
            let (x, y) = (norm(self.raw.0, self.x), norm(self.raw.1, self.y));
            match (self.was_down, self.down) {
                (false, true) => out.push(Pointer::Down(x, y)),
                (true, true) if self.moved => out.push(Pointer::Move(x, y)),
                (true, false) => out.push(Pointer::Up),
                _ => {}
            }
            self.was_down = self.down;
            self.moved = false;
        }
    }

    impl Drop for Panel {
        fn drop(&mut self) {
            unsafe { libc::close(self.fd) };
        }
    }
}
