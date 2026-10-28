//! A game's details from the web, for the back of its box (ui/back.rs): two screenshots, its
//! title screen and one in play, and what it is, from Wikipedia and Wikidata. Asked for only
//! when a game's page is opened, on a thread of its own, and kept in DSlot's cache, so the page
//! shows them at once the next time, network or not.
//!
//! - **Screenshots**: libretro's thumbnails (thumbnails.libretro.com), `Named_Titles` and
//!   `Named_Snaps`, by the system's name there and the ROM's file name, which is the No-Intro
//!   name the thumbnails go by when the ROM is named as No-Intro names it.
//! - **What it is**: Wikipedia's search for the game's name, its system and "video game"; the
//!   first page whose short description says it is a video game and whose title has the
//!   game's name in it gives the summary (Wikipedia's lead, CC BY-SA, credited on the box);
//!   its Wikidata item gives the year, the developer, the publisher and the genre (CC0).
//!
//! Every request goes by `curl`, as the updater's and RetroAchievements' do: no network is
//! simply nothing new, and what is kept stands. What wasn't found is kept too, so it isn't
//! asked for again for a week.

use crate::rom::{Console, Game};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// How long what was found stands before it is asked for again, and what wasn't.
const FOUND_FOR: Duration = Duration::from_secs(90 * 24 * 60 * 60);
const MISSED_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// How long one request may take.
const TIMEOUT_SECS: &str = "20";
const THUMBNAILS: &str = "https://thumbnails.libretro.com";
const WIKIPEDIA: &str = "https://en.wikipedia.org";
const WIKIDATA: &str = "https://www.wikidata.org/w/api.php";

/// What the game is, from Wikipedia and Wikidata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    /// Wikipedia's page title, and its lead.
    pub page: String,
    pub summary: String,
    pub year: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genre: Option<String>,
}

/// What is known of a game: its details, and its screenshots' files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Details {
    pub about: Option<About>,
    pub title_shot: Option<PathBuf>,
    pub snap: Option<PathBuf>,
}

/// What is kept for a game, beside its screenshots: when each part was last asked for.
#[derive(Default, Serialize, Deserialize)]
struct Kept {
    about: Option<About>,
    /// When the details, and the screenshots, were last asked for (Unix seconds).
    about_at: u64,
    shots_at: u64,
}

/// The system's name among libretro's thumbnails; None where there are none.
pub fn thumbnail_system(console: Console) -> Option<&'static str> {
    Some(match console {
        Console::Ds => "Nintendo - Nintendo DS",
        Console::Gba => "Nintendo - Game Boy Advance",
        Console::Gb => "Nintendo - Game Boy",
        Console::Gbc => "Nintendo - Game Boy Color",
        Console::Snes => "Nintendo - Super Nintendo Entertainment System",
        Console::Nes => "Nintendo - Nintendo Entertainment System",
        Console::Fds => "Nintendo - Family Computer Disk System",
        Console::MegaDrive => "Sega - Mega Drive - Genesis",
        Console::MasterSystem => "Sega - Master System - Mark III",
        Console::Sg1000 => "Sega - SG-1000",
        Console::GameGear => "Sega - Game Gear",
        Console::PcEngine => "NEC - PC Engine - TurboGrafx 16",
        Console::PcEngineCd => "NEC - PC Engine CD - TurboGrafx-CD",
        Console::PlayStation => "Sony - PlayStation",
        Console::Lynx => "Atari - Lynx",
        Console::NeoGeoPocket => "SNK - Neo Geo Pocket Color",
        Console::WonderSwan => "Bandai - WonderSwan",
        Console::GameWatch => "Handheld Electronic Game",
        Console::Ports | Console::Apps => return None,
    })
}

/// A file name as libretro's thumbnails spell it: these characters become `_`.
pub fn thumbnail_name(stem: &str) -> String {
    stem.chars()
        .map(|c| match c {
            '&' | '*' | '/' | ':' | '`' | '<' | '>' | '?' | '\\' | '|' | '"' => '_',
            c => c,
        })
        .collect()
}

/// Where a game's details are kept.
fn folder(cache: &Path, game: &Game) -> PathBuf {
    let stem: String = game
        .stem
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    cache.join("details").join(game.console.folder()).join(stem)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What is kept for `game`, without asking anything.
pub fn kept(cache: &Path, game: &Game) -> Details {
    let dir = folder(cache, game);
    let kept: Kept = std::fs::read_to_string(dir.join("details.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let file = |name: &str| Some(dir.join(name)).filter(|f| f.is_file());
    Details {
        about: kept.about,
        title_shot: file("title.png"),
        snap: file("snap.png"),
    }
}

/// `game`'s details: what is kept, with whatever is missing or old asked for first. Slow (a
/// few requests): on a thread.
pub fn look_up(cache: &Path, game: &Game) -> Details {
    let dir = folder(cache, game);
    let file = dir.join("details.json");
    let mut kept: Kept = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let now = now();
    let due = |at: u64, found: bool| {
        let stands = if found { FOUND_FOR } else { MISSED_FOR };
        at + stands.as_secs() <= now
    };
    let _ = std::fs::create_dir_all(&dir);
    let mut changed = false;
    let shots_found = dir.join("title.png").is_file() || dir.join("snap.png").is_file();
    if due(kept.shots_at, shots_found) {
        if let Some(system) = thumbnail_system(game.console) {
            let name = thumbnail_name(&game.stem);
            let mut reached = false;
            for (kind, to) in [("Named_Titles", "title.png"), ("Named_Snaps", "snap.png")] {
                let url = format!(
                    "{THUMBNAILS}/{}/{kind}/{}.png",
                    escape(system),
                    escape(&name)
                );
                match get(&url, &dir.join(to)) {
                    Got::Yes | Got::NotThere => reached = true,
                    Got::NoNetwork => {}
                }
            }
            // With no network, asked again next time rather than in a week.
            if reached {
                kept.shots_at = now;
                changed = true;
            }
        }
    }
    if due(kept.about_at, kept.about.is_some()) {
        // Unreached: asked again next time.
        if let Ok(about) = about(game) {
            kept.about = about.or(kept.about.take());
            kept.about_at = now;
            changed = true;
        }
    }
    if changed {
        if let Ok(text) = serde_json::to_string(&kept) {
            let part = dir.join("details.json.part");
            if std::fs::write(&part, text).is_ok() {
                let _ = std::fs::rename(&part, &file);
            }
        }
    }
    let file = |name: &str| Some(dir.join(name)).filter(|f| f.is_file());
    Details {
        about: kept.about,
        title_shot: file("title.png"),
        snap: file("snap.png"),
    }
}

/// The network couldn't be reached (or answered in nothing that could be read).
struct Unreached;

/// What a download came to.
enum Got {
    Yes,
    /// The server answered that there is no such file.
    NotThere,
    NoNetwork,
}

/// DSlot's User-Agent, as Wikimedia asks of a program: its name, version and where it lives.
fn user_agent() -> String {
    format!(
        "DSlot/{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_REPOSITORY")
    )
}

/// `url` into `to`, whole or not at all.
fn get(url: &str, to: &Path) -> Got {
    let part = to.with_extension("part");
    let status = Command::new("curl")
        .args(["-sSL", "--max-time", TIMEOUT_SECS, "-A", &user_agent()])
        .args(["-w", "%{http_code}", "-o"])
        .arg(&part)
        .arg(url)
        .output();
    let Ok(out) = status else {
        return Got::NoNetwork;
    };
    let code = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    match code.as_str() {
        "200" if std::fs::rename(&part, to).is_ok() => Got::Yes,
        "404" | "410" => {
            let _ = std::fs::remove_file(&part);
            Got::NotThere
        }
        _ => {
            let _ = std::fs::remove_file(&part);
            Got::NoNetwork
        }
    }
}

/// `url`'s answer as JSON.
fn json(url: &str) -> Result<serde_json::Value, Unreached> {
    let out = Command::new("curl")
        .args([
            "-sSfL",
            "--max-time",
            TIMEOUT_SECS,
            "-A",
            &user_agent(),
            url,
        ])
        .output()
        .map_err(|_| Unreached)?;
    if !out.status.success() {
        return Err(Unreached);
    }
    serde_json::from_slice(&out.stdout).map_err(|_| Unreached)
}

/// Percent-encodes all but the characters a URL's path or query may hold as they are.
fn escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Letters and digits only, lower case and without accents: for telling whether a page's
/// title names the game.
fn plain(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Whether Wikipedia's page `title`, described as `description`, is the game called `name`.
pub fn is_the_game(name: &str, title: &str, description: &str) -> bool {
    // "Golden Sun (video game)": the part before the brackets.
    let bare = title.split(" (").next().unwrap_or(title);
    let (name, bare) = (plain(name), plain(bare));
    // "Pokemon Emerald Version" is the page "Pokémon Emerald".
    let name = name.strip_suffix("version").unwrap_or(&name);
    !name.is_empty()
        && description.to_lowercase().contains("video game")
        && (bare == name || bare.starts_with(name))
}

/// What the game is, from Wikipedia and Wikidata: Ok(None) when they don't seem to have it.
fn about(game: &Game) -> Result<Option<About>, Unreached> {
    let name = game.name();
    let query = format!("{name} {} video game", game.console.name());
    let search = json(&format!(
        "{WIKIPEDIA}/w/api.php?action=query&list=search&srlimit=3&srprop=&format=json&srsearch={}",
        escape(&query)
    ))?;
    let titles: Vec<String> = search["query"]["search"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["title"].as_str().map(str::to_owned))
        .collect();
    for title in titles {
        let page = json(&format!(
            "{WIKIPEDIA}/api/rest_v1/page/summary/{}",
            escape(&title.replace(' ', "_"))
        ))?;
        let description = page["description"].as_str().unwrap_or("");
        if !is_the_game(name, &title, description) {
            continue;
        }
        let mut about = About {
            page: title,
            summary: page["extract"].as_str().unwrap_or("").trim().to_owned(),
            ..About::default()
        };
        if let Some(item) = page["wikibase_item"].as_str() {
            facts(item, &mut about);
        }
        return Ok(Some(about));
    }
    Ok(None)
}

/// The year, developer, publisher and genre from Wikidata's `item`, where it has them.
fn facts(item: &str, about: &mut About) {
    let Ok(claims) = json(&format!(
        "{WIKIDATA}?action=wbgetentities&props=claims&format=json&ids={item}"
    )) else {
        return;
    };
    let claims = &claims["entities"][item]["claims"];
    let values = |p: &str| -> Vec<serde_json::Value> {
        claims[p]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| c["mainsnak"]["datavalue"]["value"].clone())
            .collect()
    };
    // The earliest release: "+2001-08-01T00:00:00Z".
    about.year = values("P577")
        .iter()
        .filter_map(|v| v["time"].as_str())
        .filter_map(|t| t.get(1..5))
        .min()
        .map(str::to_owned);
    let first = |p: &str| {
        values(p)
            .first()
            .and_then(|v| v["id"].as_str().map(str::to_owned))
    };
    let (developer, publisher, genre) = (first("P178"), first("P123"), first("P136"));
    let ids: Vec<&str> = [&developer, &publisher, &genre]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    if ids.is_empty() {
        return;
    }
    let Ok(labels) = json(&format!(
        "{WIKIDATA}?action=wbgetentities&props=labels&languages=en&format=json&ids={}",
        ids.join("%7C")
    )) else {
        return;
    };
    let label = |id: &Option<String>| {
        let id = id.as_deref()?;
        labels["entities"][id]["labels"]["en"]["value"]
            .as_str()
            .map(str::to_owned)
    };
    about.developer = label(&developer);
    about.publisher = label(&publisher);
    // "role-playing video game": the kind, without the "video game" after it.
    about.genre = label(&genre).map(|g| {
        let g = g.trim_end_matches(" video game").trim_end_matches(" game");
        let mut c = g.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_names() {
        assert_eq!(
            thumbnail_name("Mario & Luigi: Superstar Saga (USA)"),
            "Mario _ Luigi_ Superstar Saga (USA)"
        );
        assert_eq!(
            escape("Nintendo - Game Boy Advance"),
            "Nintendo%20-%20Game%20Boy%20Advance"
        );
        assert_eq!(thumbnail_system(Console::Apps), None);
    }

    #[test]
    fn which_page_is_the_game() {
        assert!(is_the_game(
            "Golden Sun",
            "Golden Sun (video game)",
            "2001 video game"
        ));
        assert!(is_the_game(
            "Pokemon Emerald Version",
            "Pokémon Emerald",
            "2004 video game"
        ));
        // A page about something else, or another game of a similar name.
        assert!(!is_the_game(
            "Golden Sun",
            "Golden Sun",
            "Japanese rock band"
        ));
        assert!(!is_the_game(
            "Lighthouse Keeper",
            "Lighthouse",
            "2018 video game"
        ));
    }
}
