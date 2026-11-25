//! Fast ROM CRC32 and DS game-code readers for artwork identification.
//!
//! Zipped ROMs read the CRC32 straight out of the ZIP central directory without decompressing.
//! Loose files strip emulator headers (iNES, Lynx, Atari 7800, SNES copier) when computing
//! No-Intro checksums, and read the whole file for `art.slot-cfw.fyi` label lookups.

use crate::rom::Game;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAX_LOOSE_BYTES: u64 = 64 << 20;

/// CRC32 of a file, skipping `header` leading bytes, as 8 uppercase hex digits.
pub fn crc32_file(path: &Path, header: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    if header > 0 {
        file.seek(SeekFrom::Start(header)).ok()?;
    }
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:08X}", hasher.finalize()))
}

/// Stored CRC32 of the single matching entry in a ZIP central directory (no decompression).
/// Returns `None` if the archive holds 0 or >1 matching files.
pub fn zip_entry_crc(path: &Path, extensions: Option<&[&str]>) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let size = file.seek(SeekFrom::End(0)).ok()?;
    let tail_len = size.min(65_557) as usize;
    if tail_len < 22 {
        return None;
    }
    file.seek(SeekFrom::Start(size - tail_len as u64)).ok()?;
    let mut tail = vec![0u8; tail_len];
    file.read_exact(&mut tail).ok()?;

    let u16_le = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]) as usize;
    let u32_le = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);

    let end = (0..=tail.len() - 22)
        .rev()
        .find(|&i| u32_le(&tail, i) == 0x0605_4b50)?;
    let dir_size = u32_le(&tail, end + 12) as usize;
    let dir_offset = u32_le(&tail, end + 16);
    if dir_offset == u32::MAX
        || dir_size > 4_000_000
        || u64::from(dir_offset) + dir_size as u64 > size
    {
        return None;
    }
    file.seek(SeekFrom::Start(u64::from(dir_offset))).ok()?;
    let mut dir = vec![0u8; dir_size];
    file.read_exact(&mut dir).ok()?;

    let mut crcs = Vec::new();
    let mut offset = 0usize;
    while offset + 46 <= dir.len() && u32_le(&dir, offset) == 0x0201_4b50 {
        let name_len = u16_le(&dir, offset + 28);
        let extra_len = u16_le(&dir, offset + 30);
        let comment_len = u16_le(&dir, offset + 32);
        let next = offset + 46 + name_len + extra_len + comment_len;
        if offset + 46 + name_len > dir.len() {
            return None;
        }
        let name = String::from_utf8_lossy(&dir[offset + 46..offset + 46 + name_len]);
        let leaf = name.rsplit('/').next().unwrap_or(&name);
        if !name.starts_with("__MACOSX/") && !leaf.starts_with('.') && !name.ends_with('/') {
            let ext_ok = match extensions {
                None => true,
                Some(exts) => leaf
                    .rsplit_once('.')
                    .is_some_and(|(_, e)| exts.iter().any(|x| x.eq_ignore_ascii_case(e))),
            };
            if ext_ok {
                crcs.push(format!("{:08X}", u32_le(&dir, offset + 16)));
            }
        }
        offset = next;
    }
    if crcs.len() == 1 {
        crcs.pop()
    } else {
        None
    }
}

/// Whole-ROM CRC32 for `art.slot-cfw.fyi` (GB, GBC, GBA, and NDS — loose or single-ROM ZIP).
pub fn slot_art_checksum(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "gb" | "gbc" | "gba" | "nds" | "dsi" => {
            let size = std::fs::metadata(path).ok()?.len();
            if size == 0 || size > 512 << 20 {
                return None;
            }
            crc32_file(path, 0)
        }
        "zip" => zip_entry_crc(path, Some(&["gb", "gbc", "gba", "nds", "dsi"])),
        _ => None,
    }
}

/// No-Intro ROM CRC32 (strips iNES, Lynx, Atari 7800, and SNES copier headers; skips discs).
pub fn nointro_checksum(path: &Path) -> Option<String> {
    const DISC_OR_ARCHIVE: &[&str] = &[
        "7z", "chd", "cue", "pbp", "iso", "cso", "m3u", "bin", "gdi", "ccd", "img", "cdi", "rvz",
    ];
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext == "zip" {
        return zip_entry_crc(path, None);
    }
    if DISC_OR_ARCHIVE.contains(&ext.as_str()) {
        return None;
    }
    let size = std::fs::metadata(path).ok()?.len();
    if size == 0 || size > MAX_LOOSE_BYTES {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 10];
    let n = file.read(&mut magic).ok()?;
    let header: u64 = if n >= 4 && &magic[..4] == b"NES\x1a" {
        16
    } else if n >= 4 && &magic[..4] == b"LYNX" {
        64
    } else if n >= 10 && &magic[1..10] == b"ATARI7800" {
        128
    } else if matches!(ext.as_str(), "smc" | "sfc") && size % 1024 == 512 {
        512
    } else {
        0
    };
    crc32_file(path, header)
}

/// 4-character Nintendo DS game code (e.g. `AMCE`), from `game.code` or the ROM header at 0x0C.
pub fn ds_game_code(game: &Game) -> Option<String> {
    let valid = |s: &[u8]| {
        s.len() == 4
            && s.iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    };
    if valid(game.code.as_bytes()) {
        return Some(game.code.clone());
    }
    let ext = game.path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(ext.as_str(), "nds" | "dsi") {
        return None;
    }
    let mut file = File::open(&game.path).ok()?;
    file.seek(SeekFrom::Start(12)).ok()?;
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf).ok()?;
    valid(&buf).then(|| String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::io::Write;

    #[test]
    fn skips_ines_header_for_nointro_crc() {
        let dir = TempDir::new("crc-ines");
        let path = dir.path().join("test.nes");
        let mut raw = b"NES\x1a".to_vec();
        raw.extend_from_slice(&[0u8; 12]);
        raw.extend_from_slice(b"hello rom");
        std::fs::write(&path, &raw).unwrap();

        let expected = format!("{:08X}", crc32fast::hash(b"hello rom"));
        assert_eq!(nointro_checksum(&path), Some(expected));
    }

    #[test]
    fn reads_zip_central_directory_crc_without_decompressing() {
        let dir = TempDir::new("crc-zip");
        let path = dir.path().join("game.zip");
        let file = File::create(&path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        zw.start_file("game.gba", zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(b"gba-payload").unwrap();
        zw.finish().unwrap();

        let expected = format!("{:08X}", crc32fast::hash(b"gba-payload"));
        assert_eq!(slot_art_checksum(&path), Some(expected.clone()));
        assert_eq!(nointro_checksum(&path), Some(expected));
    }
}
