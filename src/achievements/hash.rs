//! The identity RetroAchievements gives a ROM (docs.retroachievements.org, game identification):
//! an MD5. Of the whole file for a Game Boy, Game Boy Color or Game Boy Advance game; for a DS
//! game, of its header, its two pieces of boot code and its icon/title block, since the file is
//! too big to hash whole and the rest of it changes between dumps. A zipped ROM is the file
//! inside, which is what the service sees.

use crate::rom::Console;
use md5::{Digest, Md5};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// The length of a DS header the hash covers.
const DS_HEADER: usize = 0x160;
/// The icon/title block's length.
const DS_BANNER: usize = 0xA00;
/// The most of a DS ROM the hash is ever willing to read: the boot code and the banner are
/// within the first few megabytes of any real game.
const DS_MOST: usize = 64 << 20;

/// Whether the service identifies this console's games in a way DSlot can reproduce.
pub fn supported(console: Console) -> bool {
    matches!(
        console,
        Console::Ds | Console::Gba | Console::Gb | Console::Gbc
    )
}

/// The ROM's hash, as the service spells it (32 lowercase hex digits), or none when the file
/// can't be read or isn't shaped like a game.
pub fn of(console: Console, path: &Path) -> Option<String> {
    if !supported(console) {
        return None;
    }
    with_rom(path, |r| match console {
        Console::Ds => ds(r),
        _ => whole(r),
    })?
}

/// The ROM's bytes: the file, or the first file inside a zip (the one the shelf reads the
/// header of, rom.rs).
fn with_rom<T>(path: &Path, f: impl FnOnce(&mut dyn Read) -> T) -> Option<T> {
    let file = File::open(path).ok()?;
    let zipped = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !zipped {
        let mut r = std::io::BufReader::new(file);
        return Some(f(&mut r));
    }
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip.by_index(0).ok()?;
    Some(f(&mut entry))
}

/// MD5 of everything.
fn whole(r: &mut dyn Read) -> Option<String> {
    let mut md5 = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = r.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        md5.update(&buf[..n]);
    }
    Some(hex(&md5.finalize()))
}

/// MD5 of the header, the ARM9 and ARM7 boot code and the icon/title block of a DS ROM.
fn ds(r: &mut dyn Read) -> Option<String> {
    let mut head = [0u8; DS_HEADER];
    r.read_exact(&mut head).ok()?;
    let at = |i: usize| u32::from_le_bytes(head[i..i + 4].try_into().unwrap()) as usize;
    let (arm9, arm9_len) = (at(0x20), at(0x2C));
    let (arm7, arm7_len) = (at(0x30), at(0x3C));
    let icon = at(0x68);
    let pieces = [(arm9, arm9_len), (arm7, arm7_len), (icon, DS_BANNER)];
    let end = pieces
        .iter()
        .map(|&(off, len)| off.checked_add(len))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max()?;
    if end > DS_MOST || pieces.iter().any(|&(off, _)| off < DS_HEADER && off != 0) {
        return None;
    }
    // The rest, up to the furthest piece, in one go: the pieces may be in any order.
    let mut rom = Vec::with_capacity(end);
    rom.extend_from_slice(&head);
    rom.resize(end, 0);
    r.read_exact(&mut rom[DS_HEADER..]).ok()?;
    let mut md5 = Md5::new();
    md5.update(&rom[..DS_HEADER]);
    for (off, len) in pieces {
        md5.update(&rom[off..off + len]);
    }
    Some(hex(&md5.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A DS ROM of the smallest shape: header, ARM9 then ARM7 code, the banner last.
    fn ds_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x4000 + 0x100 + 0x80 + DS_BANNER];
        rom[0x20..0x24].copy_from_slice(&0x4000u32.to_le_bytes());
        rom[0x2C..0x30].copy_from_slice(&0x100u32.to_le_bytes());
        rom[0x30..0x34].copy_from_slice(&0x4100u32.to_le_bytes());
        rom[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        rom[0x68..0x6C].copy_from_slice(&0x4180u32.to_le_bytes());
        // Something other than zeros past the header, which holds the offsets.
        for (i, b) in rom.iter_mut().enumerate().skip(DS_HEADER) {
            *b ^= (i % 251) as u8;
        }
        rom
    }

    fn expected_ds(rom: &[u8]) -> String {
        let mut md5 = Md5::new();
        md5.update(&rom[..DS_HEADER]);
        md5.update(&rom[0x4000..0x4100]);
        md5.update(&rom[0x4100..0x4180]);
        md5.update(&rom[0x4180..0x4180 + DS_BANNER]);
        hex(&md5.finalize())
    }

    #[test]
    fn ds_pieces_in_order() {
        let rom = ds_rom();
        let mut r: &[u8] = &rom;
        assert_eq!(ds(&mut r).unwrap(), expected_ds(&rom));
        // The bytes past the furthest piece don't count.
        let mut longer = rom.clone();
        longer.extend_from_slice(&[7; 4096]);
        let mut r: &[u8] = &longer;
        assert_eq!(ds(&mut r).unwrap(), expected_ds(&rom));
        // Short of the banner: no hash.
        let mut r: &[u8] = &rom[..0x4200];
        assert!(ds(&mut r).is_none());
    }

    #[test]
    fn md5_of_a_whole_gba_file_and_the_same_inside_a_zip() {
        let dir = crate::testing::TempDir::new("ra-hash");
        let gba = dir.path().join("game.gba");
        std::fs::write(&gba, b"hello GBA").unwrap();
        // echo -n 'hello GBA' | md5
        let want = of(Console::Gba, &gba).unwrap();
        assert_eq!(want.len(), 32);
        assert_eq!(want, {
            let mut m = Md5::new();
            m.update(b"hello GBA");
            hex(&m.finalize())
        });
        let zipped = dir.path().join("game.zip");
        let file = File::create(&zipped).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("Game (USA).gba", stored).unwrap();
        w.write_all(b"hello GBA").unwrap();
        w.finish().unwrap();
        assert_eq!(of(Console::Gba, &zipped).unwrap(), want);
        assert!(of(Console::Snes, &gba).is_none());
    }
}
