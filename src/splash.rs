//! The picture nibOS puts on both screens at power-on, before DSlot has started
//! (`nibos/rootfs/splash`): the DSlot logo on the top screen, the bottom one black. It is
//! written to /var/lib/dslot/splash by `dslot --write-splash` when nibOS installs DSlot.
//!
//! The file: "PSPL", a version (1) and a count, then for each screen, top first, width,
//! height, a byte count and an LZ4 block of XRGB8888 rows (all u32 little-endian).

use crate::gfx::{decode_png, Img};
use std::path::Path;

/// The DSlot logo, white and red on black (the splash, the screensaver).
pub const LOGO: &[u8] = include_bytes!("../assets/logo.png");

/// The splash's picture, for screens `w` by `h`, into the file at `path`.
pub fn write(path: &Path, w: usize, h: usize) -> Result<(), String> {
    let logo = decode_png(LOGO).ok_or("the logo is not a PNG")?;
    let mut top = Img::new(w, h);
    let bot = Img::new(w, h);
    let (x0, y0) = ((w - logo.w) / 2, (h - logo.h) / 2);
    for y in 0..logo.h {
        for x in 0..logo.w {
            let p = logo.get(x, y);
            top.px[((y0 + y) * w + x0 + x) * 4..][..4].copy_from_slice(&[p[0], p[1], p[2], 255]);
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"PSPL");
    for v in [1u32, 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for img in [&top, &bot] {
        // RGBA to XRGB8888.
        let px: Vec<u8> = img
            .px
            .chunks_exact(4)
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect();
        let block = lz4_flex::block::compress(&px);
        for v in [w as u32, h as u32, block.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&block);
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// The splash's picture as the splash program showed it: the first of `paths` that reads, top
/// screen first. DSlot puts it up in its own buffers, the same pixels, so that the buffers SDL
/// finds on screen when it starts (and puts back when its windows go) are DSlot's, which last,
/// rather than the splash program's, which go when it exits.
pub fn read(paths: &[&str]) -> Option<[Img; 2]> {
    let data = paths.iter().find_map(|p| std::fs::read(p).ok())?;
    let u32_at = |at: usize| -> Option<usize> {
        Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    if data.get(..4) != Some(b"PSPL") || u32_at(4)? != 1 || u32_at(8)? < 2 {
        return None;
    }
    let mut at = 12;
    let mut screen = || -> Option<Img> {
        let (w, h, len) = (u32_at(at)?, u32_at(at + 4)?, u32_at(at + 8)?);
        if w > 4096 || h > 4096 {
            return None;
        }
        let mut px = vec![0u8; w * h * 4];
        lz4_flex::block::decompress_into(data.get(at + 12..at + 12 + len)?, &mut px).ok()?;
        at += 12 + len;
        // XRGB8888 to RGBA.
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        Some(Img { w, h, px })
    };
    let top = screen()?;
    let bot = screen()?;
    Some([top, bot])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    /// What `write` writes, `read` reads back: the logo on the top screen, the bottom black.
    #[test]
    fn written_and_read() {
        let dir = TempDir::new("splash");
        let path = dir.path().join("splash");
        write(&path, 64 * 16, 48 * 16).unwrap();
        let [top, bot] = read(&["/nowhere/at/all", path.to_str().unwrap()]).unwrap();
        assert_eq!((top.w, top.h, bot.w, bot.h), (1024, 768, 1024, 768));
        assert!(bot.px.chunks(4).all(|p| p == [0, 0, 0, 255]));
        let logo = decode_png(LOGO).unwrap();
        let (x, y) = ((1024 - logo.w) / 2, (768 - logo.h) / 2);
        let want = logo.get(logo.w / 2, logo.h / 2);
        assert_eq!(top.get(x + logo.w / 2, y + logo.h / 2)[..3], want[..3]);
        assert!(read(&["/nowhere/at/all"]).is_none());
    }
}
