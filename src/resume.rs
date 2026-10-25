//! A game's resume point, as its emulator left it in the game's saves folder: the picture it
//! saved with it and when. Each emulator writes its own:
//!
//! - DSperate: `<game code>.auto.dss`, its picture beside it in `<game code>.auto.thumb`
//!   ("DSTHUMB1", width and height as u32 LE, then RGBA; a DS game's two screens stacked). The
//!   state holds no time: the file's is when it was saved.
//! - inkdot: `<ROM name>.auto.state`: "IKSL" 1, the time made and the play time (u64 LE
//!   seconds), the picture's length (u32 LE), the picture (PNG), the machine.
//! - inktick (Game & Watch): `<ROM name>.auto.state`, laid out as inkdot's with "ITKS" 1: the
//!   picture each screen 256 × 192, a two-screen unit's stacked.
//! - nibcase: `<ROM name>.auto.state`: "NIBSTATE", version 1, three u32 LE lengths, metadata
//!   (TOML: `core`, `created`, ...), the picture (PNG), the core's state. A resume point is
//!   each core's own: one made by another core is set aside as `<ROM name>.auto-<core>.state`
//!   when that core starts, and its own brought back.
//!
//! The one shown is the one the next start will resume: the chosen emulator's. One without a
//! picture is left out.

use crate::gfx::{self, Img};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Where to look for a game's resume point, and whose to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    /// The game's saves folder.
    pub saves: PathBuf,
    /// Its ROM's name, without its extensions.
    pub stem: String,
    pub by: By,
}

/// Which emulator the game starts in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum By {
    Dsperate,
    Inkdot,
    Inktick,
    /// nibcase, with the core it runs, by the key nibcase files its states under: the core's
    /// name in lower case, letters and digits only.
    Nibcase(String),
}

impl By {
    /// Which emulator an emulator description starts, by its command (its first word, or the
    /// script it runs) and its name ("Snes9x (nibcase)").
    pub fn of(name: &str, command: &[String]) -> Option<By> {
        let named = |n: &str| {
            command.iter().take(2).any(|w| {
                Path::new(w)
                    .file_name()
                    .is_some_and(|f| f.to_string_lossy().starts_with(n))
            })
        };
        if named("nibcase") {
            let core = name.trim_end_matches("(nibcase)").trim();
            Some(By::Nibcase(core_key(core)))
        } else if named("inkdot") {
            Some(By::Inkdot)
        } else if named("inktick") {
            Some(By::Inktick)
        } else if named("dsperate") || named("DSperate") || name.contains("DSperate") {
            Some(By::Dsperate)
        } else {
            None
        }
    }
}

fn core_key(core: &str) -> String {
    core.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// A resume point: its picture (a DS game's two screens, one over the other) and when it was saved (Unix
/// seconds).
pub struct Resume {
    pub picture: Img,
    pub saved: i64,
}

/// The resume point `wanted` describes, if there is one with a picture.
pub fn find(wanted: &Wanted) -> Option<Resume> {
    match &wanted.by {
        By::Dsperate => dsperate(&wanted.saves),
        By::Inkdot => inkdot(
            &wanted.saves.join(format!("{}.auto.state", wanted.stem)),
            b"IKSL\x01",
        ),
        By::Inktick => inkdot(
            &wanted.saves.join(format!("{}.auto.state", wanted.stem)),
            b"ITKS\x01",
        ),
        By::Nibcase(key) => {
            let current = wanted.saves.join(format!("{}.auto.state", wanted.stem));
            let aside = wanted
                .saves
                .join(format!("{}.auto-{key}.state", wanted.stem));
            // The current one, if this core made it; else this core's, set aside.
            match nibcase(&current) {
                Some((core, resume)) if core_key(&core) == *key => Some(resume),
                _ => nibcase(&aside).map(|(_, r)| r),
            }
        }
    }
}

/// A file, no bigger than `most`.
fn read(path: &Path, most: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(most)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

/// A state's first part: `header` bytes, then as many more as `more` says the header asks for
/// (its metadata and picture, not the emulator's state, which can run to megabytes).
fn head(path: &Path, header: usize, more: impl Fn(&[u8]) -> Option<usize>) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let mut bytes = vec![0; header];
    file.read_exact(&mut bytes).ok()?;
    let rest = more(&bytes)?;
    if rest > 32 * 1024 * 1024 {
        return None;
    }
    let mut tail = vec![0; rest];
    file.read_exact(&mut tail).ok()?;
    bytes.extend(tail);
    Some(bytes)
}

fn modified(path: &Path) -> Option<i64> {
    let t = fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// DSperate's: the newest `*.auto.dss` (it names them by the game's code), with its picture.
fn dsperate(saves: &Path) -> Option<Resume> {
    let state = fs::read_dir(saves)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".auto.dss"))
        .filter_map(|p| Some((modified(&p)?, p)))
        .max()?;
    let (saved, state) = state;
    let s = state.to_string_lossy();
    let thumb = PathBuf::from(format!("{}.thumb", s.trim_end_matches(".dss")));
    // Written just after the state: one older than it is an earlier state's.
    if modified(&thumb)? + 5 < saved {
        return None;
    }
    let bytes = read(&thumb, 16 + 4096 * 4096 * 4)?;
    let (w, h, px) = if bytes.starts_with(b"DSTHUMB1") {
        let (w, h) = (u32_at(&bytes, 8)?, u32_at(&bytes, 12)?);
        if w == 0 || h == 0 || w > 4096 || h > 4096 || bytes.len() - 16 != w * h * 4 {
            return None;
        }
        (w, h, &bytes[16..])
    } else if bytes.len() == 256 * 192 * 4 {
        // Before the header: the top screen alone.
        (256, 192, &bytes[..])
    } else {
        return None;
    };
    let px = px[..w * h * 4]
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    Some(Resume {
        picture: Img { w, h, px },
        saved,
    })
}

/// inkdot's, and inktick's (the same layout under its own `magic`).
fn inkdot(path: &Path, magic: &[u8; 5]) -> Option<Resume> {
    let bytes = head(path, 25, |h| u32_at(h, 21))?;
    if !bytes.starts_with(magic) {
        return None;
    }
    let saved = u64::from_le_bytes(bytes.get(5..13)?.try_into().ok()?) as i64;
    let len = u32_at(&bytes, 21)?;
    let png = bytes.get(25..25 + len).filter(|p| !p.is_empty())?;
    Some(Resume {
        picture: gfx::decode_png(png)?,
        saved,
    })
}

/// nibcase's, with the name of the core that made it.
fn nibcase(path: &Path) -> Option<(String, Resume)> {
    let bytes = head(path, 24, |h| {
        Some(u32_at(h, 12)?.min(65536) + u32_at(h, 16)?)
    })?;
    if !bytes.starts_with(b"NIBSTATE") || u32_at(&bytes, 8)? != 1 {
        return None;
    }
    let (meta, picture) = (u32_at(&bytes, 12)?, u32_at(&bytes, 16)?);
    let meta = std::str::from_utf8(bytes.get(24..24 + meta)?).ok()?;
    let png = bytes
        .get(24 + meta.len()..24 + meta.len() + picture)
        .filter(|p| !p.is_empty())?;
    #[derive(serde::Deserialize)]
    struct Meta {
        core: String,
        created: i64,
    }
    let meta: Meta = toml::from_str(meta).ok()?;
    let resume = Resume {
        picture: gfx::decode_png(png)?,
        saved: meta.created,
    };
    Some((meta.core, resume))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&vec![200; (w * h * 4) as usize])
            .unwrap();
        out
    }

    fn nibstate(core: &str, created: i64, picture: &[u8]) -> Vec<u8> {
        let meta = format!("core = \"{core}\"\nversion = \"1\"\ngame_hash = \"00\"\ncreated = {created}\nplay_seconds = 5\nstate_hash = \"00\"\n");
        let data = b"core bytes";
        let mut out = b"NIBSTATE".to_vec();
        out.extend(1u32.to_le_bytes());
        for len in [meta.len(), picture.len(), data.len()] {
            out.extend((len as u32).to_le_bytes());
        }
        out.extend(meta.as_bytes());
        out.extend(picture);
        out.extend(data);
        out
    }

    fn wanted(saves: &Path, by: By) -> Wanted {
        Wanted {
            saves: saves.to_owned(),
            stem: "Moon Bay".into(),
            by,
        }
    }

    #[test]
    fn whose_emulator() {
        let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            By::of("Snes9x (nibcase)", &words(&["/usr/bin/nibcase", "--core"])),
            Some(By::Nibcase("snes9x".into()))
        );
        assert_eq!(
            By::of("Beetle PCE Fast (nibcase)", &words(&["/usr/bin/nibcase"])),
            Some(By::Nibcase("beetlepcefast".into()))
        );
        assert_eq!(
            By::of("inkdot", &words(&["/usr/bin/inkdot", "{options}"])),
            Some(By::Inkdot)
        );
        assert_eq!(
            By::of("inktick", &words(&["/usr/bin/inktick", "{options}"])),
            Some(By::Inktick)
        );
        assert_eq!(
            By::of("DSperate", &words(&["/usr/bin/dsperate", "{options}"])),
            Some(By::Dsperate)
        );
        assert_eq!(By::of("Other", &words(&["/usr/bin/other"])), None);
    }

    #[test]
    fn nibcase_shows_the_chosen_cores() {
        let dir = TempDir::new("resume-nibcase");
        let d = dir.path();
        // The last core to run it was another: its state is current, this core's aside.
        fs::write(
            d.join("Moon Bay.auto.state"),
            nibstate("bsnes", 100, &png(4, 3)),
        )
        .unwrap();
        let mine = wanted(d, By::Nibcase("snes9x".into()));
        assert!(find(&mine).is_none());
        fs::write(
            d.join("Moon Bay.auto-snes9x.state"),
            nibstate("Snes9x", 200, &png(8, 7)),
        )
        .unwrap();
        let r = find(&mine).unwrap();
        assert_eq!((r.picture.w, r.picture.h, r.saved), (8, 7, 200));
        let theirs = find(&wanted(d, By::Nibcase("bsnes".into()))).unwrap();
        assert_eq!(theirs.saved, 100);
        // No picture: none shown.
        fs::write(d.join("Moon Bay.auto.state"), nibstate("bsnes", 100, &[])).unwrap();
        assert!(find(&wanted(d, By::Nibcase("bsnes".into()))).is_none());
    }

    #[test]
    fn inkdot_and_dsperate() {
        let dir = TempDir::new("resume-ink-ds");
        let d = dir.path();
        let picture = png(160, 144);
        let mut ink = b"IKSL\x01".to_vec();
        ink.extend(1_792_800_000u64.to_le_bytes());
        ink.extend(60u64.to_le_bytes());
        ink.extend((picture.len() as u32).to_le_bytes());
        ink.extend(&picture);
        ink.extend(b"INKDOT\0\0");
        fs::write(d.join("Moon Bay.auto.state"), ink).unwrap();
        let r = find(&wanted(d, By::Inkdot)).unwrap();
        assert_eq!((r.picture.w, r.picture.h), (160, 144));
        assert_eq!(r.saved, 1_792_800_000);
        // Not inktick's: its own magic.
        assert!(find(&wanted(d, By::Inktick)).is_none());
        let picture = png(256, 384);
        let mut tick = b"ITKS\x01".to_vec();
        tick.extend(1_792_800_100u64.to_le_bytes());
        tick.extend(30u64.to_le_bytes());
        tick.extend((picture.len() as u32).to_le_bytes());
        tick.extend(&picture);
        tick.extend(b"machine");
        fs::write(d.join("Moon Bay.auto.state"), tick).unwrap();
        let r = find(&wanted(d, By::Inktick)).unwrap();
        assert_eq!(
            (r.picture.w, r.picture.h, r.saved),
            (256, 384, 1_792_800_100)
        );
        assert!(find(&wanted(d, By::Inkdot)).is_none());

        // DSperate: no state, nothing; a state and its two-screen picture: the top screen.
        assert!(find(&wanted(d, By::Dsperate)).is_none());
        fs::write(d.join("CEYE.auto.dss"), b"DSST").unwrap();
        assert!(find(&wanted(d, By::Dsperate)).is_none());
        let mut thumb = b"DSTHUMB1".to_vec();
        thumb.extend(256u32.to_le_bytes());
        thumb.extend(384u32.to_le_bytes());
        thumb.extend(vec![9u8; 256 * 384 * 4]);
        fs::write(d.join("CEYE.auto.thumb"), thumb).unwrap();
        let r = find(&wanted(d, By::Dsperate)).unwrap();
        assert_eq!((r.picture.w, r.picture.h), (256, 384));
        assert_eq!(r.picture.px.len(), 256 * 384 * 4);
    }
}
