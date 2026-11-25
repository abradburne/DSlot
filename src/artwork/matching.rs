//! Title matching, region ranking, No-Intro / FBNeo DAT parsing, and Game & Watch lookup.
//!
//! Ported from BoxArt's `Matching.swift` and `GameAndWatch.swift` so DSlot finds the exact same
//! covers and labels on-device without ever substituting a sequel or different edition.

use crate::rom::Console;
use std::collections::{HashMap, HashSet};

/// Characters Libretro thumbnail repositories replace with `_`.
pub fn safe_name(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' | '*' | '/' | ':' | '`' | '<' | '>' | '?' | '\\' | '|' | '"' => '_',
            c => c,
        })
        .collect()
}

/// `"Tetris DX (World).gbc"` (the stem of `"Tetris DX (World).gbc.zip"`) -> `"Tetris DX (World)"`.
pub fn strip_rom_extension(stem: &str) -> &str {
    const EXTS: &[&str] = &[
        "nds", "dsi", "gba", "gb", "gbc", "nes", "fds", "sfc", "smc", "gen", "smd", "md", "sms",
        "gg", "sg", "32x", "pce", "sgx", "lnx", "ngp", "ngc", "ws", "wsc", "mgw", "iso", "cso",
        "pbp", "chd", "cue", "bin",
    ];
    let Some((head, ext)) = stem.rsplit_once('.') else {
        return stem;
    };
    if !head.is_empty() && ext.len() <= 4 && EXTS.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
        head
    } else {
        stem
    }
}

/// Parenthesised and bracketed tags of a name: `(USA)`, `(Rev 1)`, `[T-En by ...]`.
pub fn tags(name: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let close = match bytes[i] {
            b'(' => b')',
            b'[' => b']',
            _ => {
                i += 1;
                continue;
            }
        };
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && bytes[j] != close {
            j += 1;
        }
        if j < bytes.len() {
            out.push(&name[start..j]);
            i = j + 1;
        } else {
            break;
        }
    }
    out
}

fn canonical_region(token: &str) -> Option<&'static str> {
    match token.trim().to_ascii_lowercase().as_str() {
        "usa" | "u" | "us" | "america" | "north america" => Some("USA"),
        "europe" | "e" | "eu" | "eur" | "uk" | "france" | "germany" | "spain" | "italy"
        | "australia" => Some("Europe"),
        "japan" | "j" | "jp" | "jpn" | "jap" => Some("Japan"),
        "world" | "w" => Some("World"),
        _ => None,
    }
}

/// Regions named by No-Intro tags `(USA, Europe)`, scene tags `(U)`, `[EUR]`, or bare scene words.
pub fn regions(name: &str) -> HashSet<&'static str> {
    let mut result = HashSet::new();
    for tag in tags(name) {
        for part in tag.split(',') {
            if let Some(r) = canonical_region(part) {
                result.insert(r);
            }
        }
    }
    if result.is_empty() {
        let bare = strip_tags(name);
        for tok in bare.split(|c: char| !c.is_alphanumeric()) {
            if matches!(tok, "USA" | "EUR" | "JPN" | "JAP") {
                if let Some(r) = canonical_region(tok) {
                    result.insert(r);
                }
            }
        }
    }
    result
}

/// Replaces every `(...)` and `[...]` group with a space.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '(' || c == '[' {
            let close = if c == '(' { ')' } else { ']' };
            for inner in chars.by_ref() {
                if inner == close {
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Strips trailing scene release tokens (`USA`, `EUR`, `NDS-GROUP`, `v1.2`, `English`, `Patched`, etc.).
fn reduce_scene_suffix(value: &str) -> Option<String> {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    // Trim from the first patch/version word onwards.
    let mut cut = words.len();
    for (idx, &w) in words.iter().enumerate().skip(1) {
        let low = w.to_ascii_lowercase();
        let is_ver = low.starts_with('v')
            && low.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
            && low[1..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.');
        if is_ver || matches!(low.as_str(), "english" | "translated" | "patched" | "hack") {
            cut = idx;
            break;
        }
    }
    // Also trim a trailing scene tag chain like `USA`, `EUR PROPER`, `NDS-VENOM`.
    let mut end = cut;
    while end > 1 {
        let w = words[end - 1];
        let head = w.split('-').next().unwrap_or(w);
        let is_scene = matches!(
            head,
            "USA" | "EUR" | "JPN" | "JAP" | "EN" | "NDS" | "PSP" | "GBA" | "PROPER" | "READNFO"
        ) || head
            .strip_prefix("MULTi")
            .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_digit()));
        if is_scene {
            end -= 1;
        } else {
            break;
        }
    }
    if end == words.len() {
        return None;
    }
    Some(words[..end].join(" "))
}

/// Strips `, The`, `, A`, `, An` (as in `"Legend of Zelda, The"`) and a leading `The `.
fn strip_articles(part: &str) -> String {
    let mut s = part.to_owned();
    for suffix in [b", the".as_slice(), b", a".as_slice(), b", an".as_slice()] {
        let mut search_from = 0;
        while search_from + suffix.len() <= s.len() {
            let found = s.as_bytes()[search_from..]
                .windows(suffix.len())
                .position(|w| w.eq_ignore_ascii_case(suffix))
                .map(|off| search_from + off);
            let Some(pos) = found else {
                break;
            };
            let after = pos + suffix.len();
            let boundary = s[after..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
            if boundary {
                s.replace_range(pos..after, "");
                search_from = pos;
            } else {
                search_from = after;
            }
        }
    }
    let trimmed = s.trim_start();
    if trimmed
        .as_bytes()
        .get(..4)
        .is_some_and(|b| b[..3].eq_ignore_ascii_case(b"the") && b[3].is_ascii_whitespace())
    {
        return trimmed[4..].to_owned();
    }
    s
}

/// Folds common Latin diacritics and keeps lowercased alphanumeric characters only.
pub fn fold_key(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            'ý' | 'ÿ' => 'y',
            'ž' => 'z',
            'š' => 's',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Comparison keys for a title: lowercased, diacritic-folded, without punctuation, articles, or tags.
pub fn keys(name: &str, reduced: bool) -> Vec<String> {
    let mut value = strip_tags(strip_rom_extension(name)).replace(['_', '&'], " ");
    if reduced {
        let Some(shorter) = reduce_scene_suffix(&value) else {
            return Vec::new();
        };
        value = shorter;
    }
    let parts: Vec<&str> = value.split(" ~ ").collect();
    let mut candidates: Vec<&str> = Vec::with_capacity(parts.len() + 1);
    candidates.push(&value);
    if parts.len() > 1 {
        candidates.extend(parts);
    }
    let mut out = Vec::new();
    for part in candidates {
        let key = fold_key(&strip_articles(part));
        if !key.is_empty() && !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// One Libretro thumbnail set's names, indexed for exact and normalized-title lookup.
pub struct ArtworkIndex {
    exact: HashSet<String>,
    by_key: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub name: String,
    pub exact: bool,
}

impl ArtworkIndex {
    pub fn new(names: Vec<String>) -> ArtworkIndex {
        let mut exact = HashSet::with_capacity(names.len());
        let mut by_key: HashMap<String, Vec<String>> = HashMap::new();
        for name in names {
            if name.is_empty() {
                continue;
            }
            for key in keys(&name, false) {
                by_key.entry(key).or_default().push(name.clone());
            }
            exact.insert(name);
        }
        ArtworkIndex { exact, by_key }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.exact.contains(name)
    }

    pub fn match_stem(&self, stem: &str) -> Option<Match> {
        let safe = safe_name(stem);
        if self.exact.contains(&safe) {
            return Some(Match {
                name: safe,
                exact: true,
            });
        }
        let rom_regions = regions(stem);
        let own_tags: HashSet<String> = tags(stem).into_iter().map(str::to_lowercase).collect();
        for reduced in [false, true] {
            let mut seen = HashSet::new();
            let mut ranked: Vec<(&String, i32)> = Vec::new();
            for key in keys(stem, reduced) {
                if let Some(cands) = self.by_key.get(&key) {
                    for cand in cands {
                        if seen.insert(cand.as_str()) {
                            if let Some(s) = score(cand, &rom_regions, &own_tags) {
                                ranked.push((cand, s));
                            }
                        }
                    }
                }
            }
            ranked.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.len().cmp(&b.0.len()))
                    .then_with(|| a.0.cmp(b.0))
            });
            if let Some((best, _)) = ranked.first() {
                return Some(Match {
                    name: (*best).clone(),
                    exact: false,
                });
            }
        }
        None
    }
}

const UNFINISHED: &[&str] = &[
    "beta",
    "proto",
    "demo",
    "sample",
    "kiosk",
    "pirate",
    "possible proto",
];

fn is_patch(tag: &str) -> bool {
    tag.starts_with("t-") || tag.starts_with("tr ") || tag.starts_with("h ") || tag.contains("hack")
}

fn has_word(tags: &HashSet<String>, word: &str) -> bool {
    tags.iter()
        .any(|t| t == word || t.starts_with(&format!("{word} ")))
}

fn score(candidate: &str, rom_regions: &HashSet<&str>, own_tags: &HashSet<String>) -> Option<i32> {
    let cand_tags: Vec<String> = tags(candidate).into_iter().map(str::to_lowercase).collect();
    let cand_tag_set: HashSet<String> = cand_tags.iter().cloned().collect();
    for &word in UNFINISHED {
        if has_word(&cand_tag_set, word) && !has_word(own_tags, word) {
            return None;
        }
    }
    let mut score = 0i32;
    let cand_regions = regions(candidate);
    if !cand_regions.is_empty() {
        score += 20;
    }
    if !rom_regions.is_disjoint(&cand_regions) {
        score += 100;
    }
    for (pref, bonus) in [("USA", 40), ("World", 35), ("Europe", 30), ("Japan", 10)] {
        if cand_regions.contains(pref) {
            score += bonus;
            break;
        }
    }
    for tag in &cand_tags {
        if own_tags.contains(tag) {
            score += 5;
        } else if is_patch(tag) {
            score -= 15;
        } else if tag.starts_with("rev ") || tag.starts_with('v') || tag.contains("collection") {
            score -= 4;
        } else if tag.starts_with("disc ") {
            if tag != "disc 1" {
                score -= 2;
            }
        } else if regions(&format!("({tag})")).is_empty() {
            score -= 1;
        }
    }
    Some(score)
}

/// Consoles whose Libretro artwork sets overlap (e.g. a `.gb` game in `GBC/` or compatible releases).
pub fn sibling_systems(console: Console) -> &'static [Console] {
    match console {
        Console::Gb => &[Console::Gbc],
        Console::Gbc => &[Console::Gb],
        Console::GbaDs => &[Console::Gba],
        Console::MasterSystem => &[Console::Sg1000],
        Console::Sg1000 => &[Console::MasterSystem],
        Console::Nes => &[Console::Fds],
        _ => &[],
    }
}

/// Additional Libretro thumbnail repository names to search after `details::thumbnail_system(console)`.
pub fn extra_libretro_systems(console: Console) -> &'static [&'static str] {
    match console {
        Console::NeoGeoPocket => &["SNK - Neo Geo Pocket"],
        Console::WonderSwan => &["Bandai - WonderSwan Color"],
        Console::PcEngine => &["NEC - PC Engine SuperGrafx"],
        Console::Arcade => &["MAME", "SNK - Neo Geo"],
        _ => &[],
    }
}

/// Parses a No-Intro ClrMamePro `.dat`: `CRC32` (uppercase 8-hex) -> release `name`.
pub fn parse_nointro_dat(dat: &str) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let mut current: Option<String> = None;
    for line in dat.lines() {
        let text = line.trim();
        if let Some(rest) = text
            .strip_prefix("name \"")
            .and_then(|s| s.strip_suffix('"'))
        {
            current = Some(rest.to_owned());
        } else if text.starts_with("rom (") {
            if let (Some(title), Some(pos)) = (&current, text.find(" crc ")) {
                let crc = text[pos + 5..]
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                if crc.len() == 8 {
                    titles.entry(crc).or_insert_with(|| title.clone());
                }
            }
        }
    }
    titles
}

/// Parses an FBNeo ClrMamePro `.dat`: lowercase short set name (`mslug`) -> full title.
pub fn parse_arcade_dat(dat: &str) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let mut current: Option<String> = None;
    for line in dat.lines() {
        let text = line.trim();
        if let Some(rest) = text
            .strip_prefix("name \"")
            .and_then(|s| s.strip_suffix('"'))
        {
            current = Some(rest.to_owned());
        } else if let Some(rest) = text.strip_prefix("rom ( name ") {
            if let Some(title) = current.take() {
                let file = if let Some(quoted) = rest.strip_prefix('"') {
                    quoted.split('"').next().unwrap_or("")
                } else {
                    rest.split_whitespace().next().unwrap_or("")
                };
                let short = file
                    .rsplit_once('.')
                    .map_or(file, |(s, _)| s)
                    .to_ascii_lowercase();
                if !short.is_empty() {
                    titles.entry(short).or_insert(title);
                }
            }
        }
    }
    titles
}

/// Nintendo Game & Watch sets (`gnw_dkong`) -> `(real title, Libretro cover name if any)`.
pub fn game_and_watch(stem: &str) -> Option<(&'static str, Option<&'static str>)> {
    let low = stem.to_ascii_lowercase();
    GW_SETS
        .iter()
        .find(|&&(k, _, _)| k == low)
        .map(|&(_, title, art)| (title, art))
}

const GW_SETS: &[(&str, &str, Option<&str>)] = &[
    ("gnw_ball", "Ball", None),
    ("gnw_flagman", "Flagman", None),
    ("gnw_vermin", "Vermin", None),
    ("gnw_fires", "Fire (Silver)", None),
    ("gnw_judge", "Judge (purple version)", None),
    ("gnw_judgeo", "Judge (green version)", None),
    ("gnw_manholeg", "Manhole (Gold)", None),
    ("gnw_helmet", "Helmet (version CN-17)", None),
    ("gnw_helmeto", "Helmet (version CN-07)", None),
    ("gnw_lion", "Lion", None),
    (
        "gnw_pchute",
        "Parachute",
        Some("Parachute (Nintendo, Wide Screen)"),
    ),
    ("gnw_octopus", "Octopus", None),
    ("gnw_popeye", "Popeye (Wide Screen)", None),
    ("gnw_chef", "Chef", None),
    (
        "gnw_mmouse",
        "Mickey Mouse (Wide Screen)",
        Some("Mickey Mouse (Nintendo, Wide Screen)"),
    ),
    ("gnw_egg", "Egg", Some("Egg (Nintendo, Wide Screen)")),
    ("gnw_fire", "Fire (Wide Screen)", None),
    (
        "gnw_tbridge",
        "Turtle Bridge",
        Some("Turtle Bridge (Nintendo, Wide Screen)"),
    ),
    ("gnw_fireatk", "Fire Attack", None),
    (
        "gnw_stennis",
        "Snoopy Tennis",
        Some("Snoopy Tennis (Nintendo, Wide Screen)"),
    ),
    ("gnw_opanic", "Oil Panic", None),
    (
        "gnw_dkong",
        "Donkey Kong",
        Some("Donkey Kong (Nintendo, Multi Screen)"),
    ),
    ("gnw_mickdon", "Mickey & Donald", None),
    ("gnw_ghouse", "Green House", None),
    (
        "gnw_dkong2",
        "Donkey Kong II",
        Some("Donkey Kong II (Nintendo, Multi Screen)"),
    ),
    (
        "gnw_mario",
        "Mario Bros.",
        Some("Mario Bros. (Nintendo, Multi Screen)"),
    ),
    ("gnw_rshower", "Rain Shower", None),
    (
        "gnw_lboat",
        "Life Boat",
        Some("Lifeboat (Nintendo, Multi Screen)"),
    ),
    ("gnw_pinball", "Pinball", None),
    ("gnw_bjack", "Black Jack", None),
    ("gnw_squish", "Squish", None),
    ("gnw_bsweep", "Bomb Sweeper", None),
    ("gnw_sbuster", "Safe Buster", None),
    ("gnw_gcliff", "Gold Cliff", None),
    ("gnw_zelda", "Zelda", None),
    ("gnw_dkjr", "Donkey Kong Jr. (New Wide Screen)", None),
    (
        "gnw_mariocm",
        "Mario's Cement Factory (New Wide Screen)",
        None,
    ),
    ("gnw_manhole", "Manhole (New Wide Screen)", None),
    ("gnw_tfish", "Tropical Fish", None),
    ("gnw_smb", "Super Mario Bros. (Crystal Screen)", None),
    ("gnw_smbn", "Super Mario Bros. (New Wide Screen)", None),
    ("gnw_climber", "Climber (Crystal Screen)", None),
    ("gnw_climbern", "Climber (New Wide Screen)", None),
    ("gnw_bfight", "Balloon Fight (Crystal Screen)", None),
    ("gnw_bfightn", "Balloon Fight (New Wide Screen)", None),
    ("gnw_mariotj", "Mario The Juggler", None),
    (
        "gnw_mariocmt",
        "Mario's Cement Factory (Table Top, version CM-72)",
        None,
    ),
    (
        "gnw_mariocmta",
        "Mario's Cement Factory (Table Top, version CM-72A)",
        None,
    ),
    (
        "gnw_snoopyp",
        "Snoopy (Panorama Screen)",
        Some("Snoopy (Nintendo, Panorama Screen)"),
    ),
    ("gnw_popeyep", "Popeye (Panorama Screen)", None),
    (
        "gnw_dkjrp",
        "Donkey Kong Jr. (Panorama Screen)",
        Some("Donkey Kong Jr. (Nintendo, Panorama Screen)"),
    ),
    (
        "gnw_mbaway",
        "Mario's Bombs Away",
        Some("Mario's Bombs Away (Nintendo, Panorama Screen)"),
    ),
    (
        "gnw_mmousep",
        "Mickey Mouse (Panorama Screen)",
        Some("Mickey Mouse (Nintendo, Panorama Screen)"),
    ),
    (
        "gnw_dkcirc",
        "Donkey Kong Circus",
        Some("Donkey Kong Circus (Nintendo, Panorama Screen)"),
    ),
    ("gnw_ssparky", "Spitball Sparky", None),
    ("gnw_cgrab", "Crab Grab", None),
    ("gnw_boxing", "Boxing", None),
    ("gnw_dkong3", "Donkey Kong 3", None),
    ("gnw_dkhockey", "Donkey Kong Hockey", None),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_region_and_never_substitutes_sequels() {
        let index = ArtworkIndex::new(vec![
            "Mega Man 2 (USA)".into(),
            "Mega Man 3 (USA)".into(),
            "Legend of Zelda, The - Link's Awakening (USA, Europe)".into(),
            "Legend of Zelda, The - Link's Awakening (Japan)".into(),
            "Pokemon - Emerald Version (USA, Europe)".into(),
            "Chrono Trigger (USA) (Beta)".into(),
        ]);
        // Never substitutes a sequel.
        assert_eq!(index.match_stem("Mega Man 4 (USA)"), None);
        // Exact match wins.
        assert_eq!(
            index.match_stem("Mega Man 2 (USA)"),
            Some(Match {
                name: "Mega Man 2 (USA)".into(),
                exact: true
            })
        );
        // Article reordering + region preference.
        assert_eq!(
            index.match_stem("The Legend of Zelda - Link's Awakening (J)"),
            Some(Match {
                name: "Legend of Zelda, The - Link's Awakening (Japan)".into(),
                exact: false
            })
        );
        // Prototype never matches a retail ROM.
        assert_eq!(index.match_stem("Chrono Trigger (USA)"), None);
        assert!(index.match_stem("Chrono Trigger (USA) (Beta)").is_some());
        // Multi-byte UTF-8 titles never panic in article stripping.
        assert_eq!(keys("3×3 EYES 三只眼變成", false), vec!["33eyes三只眼變成"]);
        assert_eq!(keys("Jūōki (Japan)", false), vec!["jūōki"]);
        assert_eq!(keys("Ōkami, The (Japan)", false), vec!["ōkami"]);
    }

    #[test]
    fn parses_nointro_and_arcade_dats() {
        let dat = r#"
game (
    name "Final Fantasy VI Advance (USA)"
    rom ( name "Final Fantasy VI Advance (USA).gba" size 8388608 crc 1a2b3c4d md5 00 )
)
"#;
        let map = parse_nointro_dat(dat);
        assert_eq!(
            map.get("1A2B3C4D").map(String::as_str),
            Some("Final Fantasy VI Advance (USA)")
        );

        let fbneo = r#"
game (
    name "Metal Slug - Super Vehicle-001"
    rom ( name mslug.zip size 1234 )
)
"#;
        let arc = parse_arcade_dat(fbneo);
        assert_eq!(
            arc.get("mslug").map(String::as_str),
            Some("Metal Slug - Super Vehicle-001")
        );
    }
}
