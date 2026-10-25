//! SNES cartridge metadata, from PVSnesLib's SNES ROM Header documentation (docs/snes.md).
//! Only header-sized reads for loose ROMs; ZIP members are streamed, never extracted to disk.

use super::{maker, tidy};
use crate::gfx::Img;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

// Expanded header, title, attributes and vectors, at LoROM, HiROM and ExHiROM offsets.
const OFFSETS: [u64; 3] = [0x7FB0, 0xFFB0, 0x40FFB0];
const HEADER_LEN: usize = 0x50;

pub(super) fn parse(path: &Path, stem: &str) -> (String, Vec<String>, Option<Img>) {
    let mut title = vec![tidy(stem)];
    let Some(head) = header(path) else {
        return (String::new(), title, None);
    };
    // Older carts have only the one-byte licensee. The expanded header is marked by $33.
    let expanded = head[0x2A] == 0x33;
    let licensee = if expanded {
        String::from_utf8_lossy(&head[..2]).into_owned()
    } else {
        format!("{:02X}", head[0x2A])
    };
    if let Some(maker) = maker(&licensee) {
        title.push(maker.to_owned());
    }
    if stem.is_empty() {
        // JIS X 0201: ASCII and half-width katakana; do not replace the file's fuller title.
        title[0] = head[0x10..0x25]
            .iter()
            .map(|&b| match b {
                0xA1..=0xDF => char::from_u32(0xFF61 + u32::from(b - 0xA1)).unwrap(),
                _ => char::from(b),
            })
            .collect::<String>()
            .trim()
            .to_owned();
    }
    let code = if expanded && head[2..6].iter().all(u8::is_ascii_alphanumeric) {
        String::from_utf8_lossy(&head[2..6]).into_owned()
    } else {
        String::new()
    };
    (code, title, None)
}

/// Whether a SNES game was made for North America (the US or Canada), where carts had the US
/// shape: by its header's destination code, else by the region in its file's name (No-Intro's
/// "(USA)", GoodTools' "(U)"). `None` when neither says.
pub fn north_american(path: &Path, stem: &str) -> Option<bool> {
    // The destination code, at $FFD9: 1 is North America, 15 Canada.
    if let Some(head) = header(path).filter(|h| h[0x29] <= 0x14) {
        return Some(matches!(head[0x29], 0x01 | 0x0F));
    }
    let tags: Vec<&str> = stem
        .split(['(', ')', '[', ']', ','])
        .map(str::trim)
        .collect();
    let has = |names: &[&str]| tags.iter().any(|t| names.contains(t));
    if has(&["USA", "U", "Canada", "US"]) {
        Some(true)
    } else if has(&[
        "Japan",
        "J",
        "Europe",
        "E",
        "Germany",
        "France",
        "Spain",
        "Italy",
        "Australia",
        "Korea",
    ]) {
        Some(false)
    } else {
        None
    }
}

fn header(path: &Path) -> Option<[u8; HEADER_LEN]> {
    let mut file = File::open(path).ok()?;
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        let size = file.metadata().ok()?.len();
        return choose(size, |offset, bytes| {
            file.seek(SeekFrom::Start(offset)).ok()?;
            file.read_exact(bytes).ok()
        });
    }
    let mut zip = zip::ZipArchive::new(file).ok()?;
    for index in 0..zip.len() {
        let mut rom = zip.by_index(index).ok()?;
        let ext = Path::new(rom.name()).extension().and_then(|e| e.to_str());
        if rom.is_dir()
            || !ext.is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "sfc" | "smc"))
        {
            continue;
        }
        let mut position = 0;
        return choose(rom.size(), |offset, bytes| {
            let skip = offset.checked_sub(position)?;
            let skipped = std::io::copy(&mut (&mut rom).take(skip), &mut std::io::sink()).ok()?;
            if skipped != skip {
                return None;
            }
            rom.read_exact(bytes).ok()?;
            position = offset + bytes.len() as u64;
            Some(())
        });
    }
    None
}

fn choose(
    size: u64,
    mut read: impl FnMut(u64, &mut [u8]) -> Option<()>,
) -> Option<[u8; HEADER_LEN]> {
    // Copier headers are 512 bytes before the cartridge data, independent of .sfc/.smc.
    let copier = if size % 1024 == 512 { 512 } else { 0 };
    OFFSETS
        .into_iter()
        .filter_map(|offset| {
            if offset + copier + HEADER_LEN as u64 > size {
                return None;
            }
            let mut bytes = [0; HEADER_LEN];
            read(offset + copier, &mut bytes)?;
            let mode = bytes[0x25] & !0x10; // FastROM does not change the header's position.
            let matches = match offset {
                0x7FB0 => matches!(mode, 0x20 | 0x22 | 0x23),
                0xFFB0 => mode == 0x21,
                _ => mode == 0x25,
            };
            let title = &bytes[0x10..0x25];
            if !matches
                || !title.iter().any(|&b| b != b' ')
                || !title.iter().all(|b| matches!(b, 0x20..=0x7E | 0xA1..=0xDF))
            {
                return None;
            }
            // Patched games and homebrew may have stale checksums. Use them as evidence,
            // never as a condition for listing a game, and prefer a plausible reset vector.
            let le16 = |at| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
            let checksum = le16(0x2C) ^ le16(0x2E) == 0xFFFF && le16(0x2E) != 0;
            let score = u8::from(checksum) * 8 + u8::from(le16(0x4C) >= 0x8000) * 4;
            Some((score, bytes))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, bytes)| bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::Console;
    use crate::testing::TempDir;
    use std::io::Write;

    fn rom(offset: usize, mode: u8, copier: usize, expanded: bool) -> Vec<u8> {
        let mut rom = vec![0; (offset + HEADER_LEN).next_power_of_two() + copier];
        let h = &mut rom[offset + copier..][..HEADER_LEN];
        h[0x10..0x25].fill(b' ');
        h[0x10..0x18].copy_from_slice(b"MOON BAY");
        h[0x25] = mode;
        h[0x2A] = if expanded { 0x33 } else { 0x01 };
        h[0x2C..0x30].copy_from_slice(&[0xCB, 0xED, 0x34, 0x12]);
        h[0x4C..0x4E].copy_from_slice(&0x8000u16.to_le_bytes());
        if expanded {
            h[..6].copy_from_slice(b"A4AMBE");
        }
        rom
    }

    #[test]
    fn mappings_and_copier_headers() {
        let dir = TempDir::new("snes-headers");
        for (offset, mode) in [
            (0x7FB0, 0x20),
            (0xFFB0, 0x31),
            (0x40FFB0, 0x35),
            (0x7FB0, 0x23),
        ] {
            for copier in [0, 512] {
                let path = dir.path().join("Moon Bay (USA).smc");
                std::fs::write(&path, rom(offset, mode, copier, true)).unwrap();
                let (code, title, _) = parse(&path, "Moon Bay (USA)");
                assert_eq!(code, "AMBE", "{offset:X}, {copier}");
                assert_eq!(title, ["Moon Bay", "Konami"]);
                assert_eq!(parse(&path, "").1[0], "MOON BAY");
            }
        }
    }

    #[test]
    fn zipped_rom_after_readme_and_directory() {
        let dir = TempDir::new("snes-zip");
        for method in [
            zip::CompressionMethod::Stored,
            zip::CompressionMethod::Deflated,
        ] {
            let path = dir.path().join("Moon Bay.sfc.zip");
            let mut archive = zip::ZipWriter::new(File::create(&path).unwrap());
            let options = zip::write::SimpleFileOptions::default().compression_method(method);
            archive.start_file("readme.txt", options).unwrap();
            archive.write_all(b"A homebrew game").unwrap();
            archive.add_directory("game/", options).unwrap();
            archive.start_file("game/Moon Bay.SMC", options).unwrap();
            archive.write_all(&rom(0xFFB0, 0x21, 512, false)).unwrap();
            archive.finish().unwrap();
            let (code, title, _) = parse(&path, "Moon Bay");
            assert_eq!(code, "");
            assert_eq!(title, ["Moon Bay", "Nintendo"]);
            assert_eq!(
                super::super::read_one(&path).unwrap().console,
                Console::Snes
            );
        }
    }

    #[test]
    fn malformed_headers_fall_back_to_filename() {
        let dir = TempDir::new("snes-bad");
        let path = dir.path().join("Moon Bay.sfc");
        for bytes in [vec![], vec![0xFF; 0x10000], rom(0x7FB0, 0x21, 0, true)] {
            std::fs::write(&path, bytes).unwrap();
            let (code, title, _) = parse(&path, "Moon Bay (USA)");
            assert_eq!(code, "");
            assert_eq!(title, ["Moon Bay"]);
        }
    }
}
