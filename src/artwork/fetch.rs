//! Online artwork lookup and atomic PNG saving for DSlot.
//!
//! Fetches missing box art (`Imgs/<stem>.png`), cart labels (`Labels/<stem>.png`), and finished
//! cart / UMD scans (`Carts/<stem>.png`) right beside each ROM without ever touching ROM files.
//! Uses `curl` (same as `details.rs` and `achievements/service.rs`) with zero API keys.

use super::checksum;
use super::launchbox::{self, PlatformIndex};
use super::matching::{self, ArtworkIndex};
use crate::art;
use crate::gfx::{self, Img};
use crate::rom::{Console, Game};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

const MAX_EDGE: usize = 512;
const TIMEOUT_SECS: &str = "20";
const LIST_TIMEOUT_SECS: &str = "40";
const SLOT_ART_BASE: &str = "https://art.slot-cfw.fyi/";

/// Which artwork to collect.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ArtKind {
    #[default]
    Both,
    BoxOnly,
    CartOnly,
}

impl ArtKind {
    pub fn label(self) -> &'static str {
        match self {
            ArtKind::Both => "Box & cart",
            ArtKind::BoxOnly => "Box art",
            ArtKind::CartOnly => "Cart & disc",
        }
    }

    pub fn step(self, dir: i32) -> ArtKind {
        const ALL: [ArtKind; 3] = [ArtKind::Both, ArtKind::BoxOnly, ArtKind::CartOnly];
        let i = ALL.iter().position(|&k| k == self).unwrap_or(0) as i32;
        ALL[(i + dir).rem_euclid(ALL.len() as i32) as usize]
    }

    pub fn wants_box(self) -> bool {
        matches!(self, ArtKind::Both | ArtKind::BoxOnly)
    }

    pub fn wants_cart(self) -> bool {
        matches!(self, ArtKind::Both | ArtKind::CartOnly)
    }
}

/// Destination subfolder beside the ROM.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Folder {
    Imgs,
    Labels,
    Carts,
}

impl Folder {
    pub fn name(self) -> &'static str {
        match self {
            Folder::Imgs => "Imgs",
            Folder::Labels => "Labels",
            Folder::Carts => "Carts",
        }
    }
}

/// Outcome of fetching artwork for one game.
#[derive(Clone, Debug, Default)]
pub struct GameOutcome {
    pub box_saved: bool,
    pub cart_saved: bool,
    pub missing: bool,
    pub error: Option<String>,
}

/// Whether `game` can have online artwork fetched (excludes folders, PortMaster ports, and apps).
pub fn eligible(game: &Game) -> bool {
    !game.folder && !game.gone && !matches!(game.console, Console::Ports | Console::Apps)
}

/// Whether DSlot displays a cartridge or disc for `console`.
pub fn supports_cart(console: Console) -> bool {
    art::has_shell(console)
}

/// Whether `game` already has saved box art beside the ROM (`Imgs/<stem>.png|jpg|jpeg`).
pub fn has_box(game: &Game) -> bool {
    let dir = game.dir().join("Imgs");
    for ext in ["png", "jpg", "jpeg"] {
        if dir.join(format!("{}.{ext}", game.stem)).is_file() {
            return true;
        }
    }
    false
}

/// Whether `game` already has a saved cart label or finished cart beside the ROM (or in Cart Studio's layout).
pub fn has_cart(game: &Game) -> bool {
    if game.art("Labels").is_file() || game.art("Carts").is_file() {
        return true;
    }
    let cards = art::studio_cards(game);
    art::studio_labels(game, &cards)
        .into_iter()
        .any(|(p, _)| p.is_file())
}

/// Whether `game` is missing any artwork requested by `kind`.
pub fn needs_art(game: &Game, kind: ArtKind) -> bool {
    if !eligible(game) {
        return false;
    }
    let need_box = kind.wants_box() && !has_box(game);
    let need_cart = kind.wants_cart() && supports_cart(game.console) && !has_cart(game);
    need_box || need_cart
}

/// Whether `curl` is present on PATH (checked lazily on first use, never at startup).
pub fn curl_available() -> bool {
    static HAS_CURL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS_CURL.get_or_init(|| {
        Command::new("curl")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

#[derive(Default)]
struct CacheState {
    /// Libretro `Named_Boxarts` indexes by repo name (`None` = unavailable this run).
    libretro_indexes: HashMap<String, Option<Arc<ArtworkIndex>>>,
    /// No-Intro CRC -> name maps by Libretro system name.
    nointro: HashMap<String, Arc<HashMap<String, String>>>,
    /// FBNeo short set -> full arcade title.
    arcade: Option<Arc<HashMap<String, String>>>,
    /// `art.slot-cfw.fyi/index.json` CRC32 -> `support-texture` relative path.
    slot_labels: Option<Arc<HashMap<String, String>>>,
    /// LaunchBox per-console indexes (`None` = no `.tsv.z` for that console).
    launchbox: HashMap<String, Option<Arc<PlatformIndex>>>,
}

/// Shared artwork lookup client safe to call from multiple worker threads.
pub struct Service {
    cache_dir: PathBuf,
    art_index_dirs: Vec<PathBuf>,
    state: Mutex<CacheState>,
    /// Serializes remote index downloads so parallel workers don't stampede GitHub / slot-cfw.
    index_lock: Mutex<()>,
}

impl Service {
    pub fn new(cache: &Path, art_index_dirs: Vec<PathBuf>) -> Service {
        Service {
            cache_dir: cache.join("artwork"),
            art_index_dirs,
            state: Mutex::new(CacheState::default()),
            index_lock: Mutex::new(()),
        }
    }

    /// Drops cached LaunchBox, Libretro, No-Intro, FBNeo, and slot-cfw index maps from RAM once
    /// a batch completes; on-disk caches stay intact for future runs.
    pub fn clear_caches(&self) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = CacheState::default();
    }

    /// Fetches and saves requested artwork for `game` into `Imgs/`, `Labels/`, and/or `Carts/`
    /// beside the ROM. Never overwrites existing files unless `replace` is true.
    pub fn process_game(
        &self,
        game: &Game,
        kind: ArtKind,
        replace: bool,
        cancel: &AtomicBool,
    ) -> GameOutcome {
        if !eligible(game) || cancel.load(Ordering::Relaxed) {
            return GameOutcome::default();
        }
        let want_box = kind.wants_box() && (replace || !has_box(game));
        let want_cart =
            kind.wants_cart() && supports_cart(game.console) && (replace || !has_cart(game));
        if !want_box && !want_cart {
            return GameOutcome::default();
        }

        let mut out = GameOutcome::default();
        let mut tried = false;

        if want_box && !cancel.load(Ordering::Relaxed) {
            tried = true;
            match self.fetch_box(game, cancel) {
                Ok(Some((img, source))) => {
                    match save_image(game, Folder::Imgs, img, true, replace) {
                        Ok(()) => {
                            out.box_saved = true;
                            eprintln!("dslot: box art saved for {} ({source})", game.stem);
                        }
                        Err(e) => out.error = Some(e),
                    }
                }
                Ok(None) => {}
                Err(e) => out.error = Some(e),
            }
        }

        if want_cart && !cancel.load(Ordering::Relaxed) {
            tried = true;
            match self.fetch_cart(game, cancel) {
                Ok(Some((img, folder, source))) => {
                    match save_image(game, folder, img, false, replace) {
                        Ok(()) => {
                            if replace {
                                // Clear the opposite cart/label folder so the newly fetched asset wins.
                                let other = match folder {
                                    Folder::Labels => Folder::Carts,
                                    Folder::Carts => Folder::Labels,
                                    Folder::Imgs => Folder::Imgs,
                                };
                                let _ = std::fs::remove_file(game.art(other.name()));
                            }
                            out.cart_saved = true;
                            eprintln!(
                                "dslot: {} saved for {} ({source})",
                                folder.name(),
                                game.stem
                            );
                        }
                        Err(e) => {
                            if out.error.is_none() {
                                out.error = Some(e);
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    if out.error.is_none() {
                        out.error = Some(e);
                    }
                }
            }
        }

        if tried && !out.box_saved && !out.cart_saved && out.error.is_none() {
            out.missing = true;
        }
        out
    }

    /// Fetches box art for `game` (returns decoded image + source description). Also usable by
    /// `details.rs` when opening a game's page.
    pub fn fetch_box(
        &self,
        game: &Game,
        cancel: &AtomicBool,
    ) -> Result<Option<(Img, String)>, String> {
        let mut last_err: Option<String> = None;
        let raw_stem = matching::strip_rom_extension(&game.stem);
        let mut stem = raw_stem.to_owned();
        let mut gw_real_title: Option<&str> = None;

        if game.console == Console::Arcade {
            // DSlot already resolves arcade game names via emulator lists; fall back to FBNeo DAT.
            if !game.name().is_empty() && game.name() != game.stem {
                stem = game.name().to_owned();
            } else if let Some(title) = self.arcade_title(&stem) {
                stem = title;
            }
        } else if game.console == Console::GameWatch {
            if let Some((title, cover)) = matching::game_and_watch(&stem) {
                gw_real_title = Some(title);
                if let Some(cover) = cover {
                    stem = cover.to_owned();
                }
            }
        }

        let safe = matching::safe_name(&stem);
        let primary_sys = crate::details::thumbnail_system(game.console);

        // 1. Libretro exact filename (skip if we already have the name list and know it's absent).
        if let Some(sys) = primary_sys {
            let repo = sys.replace(' ', "_");
            let known_missing = self
                .state
                .lock()
                .unwrap()
                .libretro_indexes
                .get(&repo)
                .and_then(|opt| opt.as_ref())
                .is_some_and(|idx| !idx.contains(&safe));
            if !known_missing && !cancel.load(Ordering::Relaxed) {
                match fetch_libretro_box(&repo, &safe) {
                    Ok(Some(img)) => return Ok(Some((img, "Libretro · exact".into()))),
                    Ok(None) => {}
                    Err(e) => last_err = Some(e),
                }
            }
        }

        // 2. GameTDB DS cover by exact 4-char game code (medium JPG, then small PNG).
        if game.console == Console::Ds && !cancel.load(Ordering::Relaxed) {
            if let Some(code) = checksum::ds_game_code(game) {
                let regions = ds_regions(&code);
                for (cover, ext) in [("coverM", "jpg"), ("coverS", "png")] {
                    for region in &regions {
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(None);
                        }
                        let url =
                            format!("https://art.gametdb.com/ds/{cover}/{region}/{code}.{ext}");
                        match fetch_image(&url, 1) {
                            Ok(Some(img)) => {
                                return Ok(Some((img, format!("GameTDB · {code} · {region}"))));
                            }
                            Ok(None) => {}
                            Err(e) => {
                                last_err = Some(e);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // 3. No-Intro ROM CRC32 -> exact Libretro filename (console + siblings).
        if !matches!(game.console, Console::Arcade | Console::GameWatch)
            && !cancel.load(Ordering::Relaxed)
        {
            if let Some(crc) = checksum::nointro_checksum(&game.path) {
                for sys in libretro_systems(game.console) {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(None);
                    }
                    let Some(name) = self.nointro_title(sys, &crc) else {
                        continue;
                    };
                    let repo = sys.replace(' ', "_");
                    match fetch_libretro_box(&repo, &matching::safe_name(&name)) {
                        Ok(Some(img)) => {
                            return Ok(Some((img, format!("Libretro · CRC {crc}"))));
                        }
                        Ok(None) => {
                            // Found the canonical dump name, so use it for title matching below.
                            stem = name;
                            break;
                        }
                        Err(e) => last_err = Some(e),
                    }
                }
            }
        }

        // 4. Libretro GitHub tree name list -> normalized title + region match across siblings.
        for sys in libretro_systems(game.console) {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let Some(index) = self.libretro_index(sys) else {
                continue;
            };
            let Some(m) = index.match_stem(&stem) else {
                continue;
            };
            if m.exact && Some(sys) == primary_sys {
                continue;
            }
            let repo = sys.replace(' ', "_");
            match fetch_libretro_box(&repo, &m.name) {
                Ok(Some(img)) => return Ok(Some((img, format!("Libretro · {}", m.name)))),
                Ok(None) => {}
                Err(e) => last_err = Some(e),
            }
        }

        // 5. LaunchBox Box - Front fallback (huge win for Game & Watch, FDS, Pico, PSP, etc.).
        let lb_stem = gw_real_title.unwrap_or(&stem);
        if !cancel.load(Ordering::Relaxed) {
            if let Some(url) = self.launchbox_url(game.console, lb_stem, false) {
                match fetch_image(&url, 1) {
                    Ok(Some(img)) => return Ok(Some((img, "LaunchBox · box".into()))),
                    Ok(None) => {}
                    Err(e) => last_err = Some(e),
                }
            }
        }

        if let Some(err) = last_err {
            Err(err)
        } else {
            Ok(None)
        }
    }

    /// Fetches cart label (`Labels/`) or finished cart / UMD scan (`Carts/`) for `game`.
    pub fn fetch_cart(
        &self,
        game: &Game,
        cancel: &AtomicBool,
    ) -> Result<Option<(Img, Folder, String)>, String> {
        let mut last_err: Option<String> = None;

        // 1. Nintendo DS: GameTDB label -> art.slot-cfw.fyi (.webp) -> LaunchBox Cart - Front.
        if game.console == Console::Ds {
            if let Some(code) = checksum::ds_game_code(game) {
                for region in ds_regions(&code) {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(None);
                    }
                    let url = format!("https://art.gametdb.com/ds/label/{region}/{code}.png");
                    match fetch_image(&url, 1) {
                        Ok(Some(img)) => {
                            return Ok(Some((
                                img,
                                Folder::Labels,
                                format!("GameTDB · {code} · {region}"),
                            )));
                        }
                        Ok(None) => {}
                        Err(e) => {
                            last_err = Some(e);
                            break;
                        }
                    }
                }
            }
            if !cancel.load(Ordering::Relaxed) {
                match self.fetch_slot_label(game) {
                    Ok(Some((img, crc))) => {
                        return Ok(Some((img, Folder::Labels, format!("slot art · {crc}"))));
                    }
                    Ok(None) => {}
                    Err(e) => last_err = Some(e),
                }
            }
        }

        // 2. Game Boy, Game Boy Color, Game Boy Advance (+ GBA DS): art.slot-cfw.fyi (.webp) by CRC32.
        if matches!(
            game.console,
            Console::Gb | Console::Gbc | Console::Gba | Console::GbaDs
        ) && !cancel.load(Ordering::Relaxed)
        {
            match self.fetch_slot_label(game) {
                Ok(Some((img, crc))) => {
                    return Ok(Some((img, Folder::Labels, format!("slot art · {crc}"))));
                }
                Ok(None) => {}
                Err(e) => last_err = Some(e),
            }
        }

        // 3. LaunchBox Cart - Front (all cartridge consoles) or Disc (PS1, Mega-CD, PCE CD, PSP).
        if !cancel.load(Ordering::Relaxed) {
            let raw_stem = matching::strip_rom_extension(&game.stem);
            if let Some(url) = self.launchbox_url(game.console, raw_stem, true) {
                match fetch_image(&url, 1) {
                    Ok(Some(img)) => {
                        // Optical CDs go to Labels/ so DSlot renders glossy CD sheen + multi-disc badge;
                        // PSP UMDs and all cartridge scans are complete physical media -> Carts/.
                        let folder = match game.console {
                            Console::PlayStation | Console::MegaCd | Console::PcEngineCd => {
                                Folder::Labels
                            }
                            _ => Folder::Carts,
                        };
                        return Ok(Some((img, folder, "LaunchBox · media".into())));
                    }
                    Ok(None) => {}
                    Err(e) => last_err = Some(e),
                }
            }
        }

        if let Some(err) = last_err {
            Err(err)
        } else {
            Ok(None)
        }
    }

    /// Looks up a sticker scan in `art.slot-cfw.fyi` by whole-ROM CRC32, trying `.webp` then `.png`.
    fn fetch_slot_label(&self, game: &Game) -> Result<Option<(Img, String)>, String> {
        let Some(crc) = checksum::slot_art_checksum(&game.path) else {
            return Ok(None);
        };
        let Some(index) = self.slot_label_index() else {
            return Ok(None);
        };
        let Some(rel_path) = index.get(&crc) else {
            return Ok(None);
        };
        // art.slot-cfw.fyi migrated images to .webp while keeping .png paths in index.json.
        let mut urls = Vec::new();
        if let Some(base) = rel_path.strip_suffix(".png") {
            urls.push(format!("{SLOT_ART_BASE}{base}.webp"));
        }
        urls.push(format!("{SLOT_ART_BASE}{rel_path}"));
        for url in urls {
            match fetch_image(&url, 1) {
                Ok(Some(img)) => return Ok(Some((img, crc))),
                Ok(None) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    fn slot_label_index(&self) -> Option<Arc<HashMap<String, String>>> {
        if let Some(idx) = self.state.lock().unwrap().slot_labels.clone() {
            return Some(idx);
        }
        let _guard = self.index_lock.lock().unwrap();
        if let Some(idx) = self.state.lock().unwrap().slot_labels.clone() {
            return Some(idx);
        }
        let bytes = self.cached_bytes(
            "slot-art-index.json",
            Duration::from_secs(7 * 86_400),
            &format!("{SLOT_ART_BASE}index.json"),
        )?;
        let parsed: HashMap<String, serde_json::Value> = serde_json::from_slice(&bytes).ok()?;
        let map: HashMap<String, String> = parsed
            .into_iter()
            .filter_map(|(k, v)| {
                let tex = v.get("support-texture")?.as_str()?.to_owned();
                Some((k.to_ascii_uppercase(), tex))
            })
            .collect();
        let arc = Arc::new(map);
        self.state.lock().unwrap().slot_labels = Some(arc.clone());
        Some(arc)
    }

    fn launchbox_url(&self, console: Console, stem: &str, want_cart: bool) -> Option<String> {
        let mut consoles = vec![console];
        consoles.extend_from_slice(matching::sibling_systems(console));
        for c in consoles {
            let Some(key) = launchbox::console_key(c) else {
                continue;
            };
            if let Some(idx) = self.launchbox_index(key) {
                if let Some(url) = idx.lookup(stem, want_cart) {
                    return Some(url);
                }
            }
        }
        None
    }

    fn launchbox_index(&self, key: &str) -> Option<Arc<PlatformIndex>> {
        if let Some(entry) = self.state.lock().unwrap().launchbox.get(key) {
            return entry.clone();
        }
        let loaded = launchbox::find_file(&self.art_index_dirs, key)
            .and_then(|p| PlatformIndex::load(&p))
            .map(Arc::new);
        self.state
            .lock()
            .unwrap()
            .launchbox
            .insert(key.to_owned(), loaded.clone());
        loaded
    }

    fn libretro_index(&self, system: &str) -> Option<Arc<ArtworkIndex>> {
        let repo = system.replace(' ', "_");
        if let Some(cached) = self.state.lock().unwrap().libretro_indexes.get(&repo) {
            return cached.clone();
        }
        let _guard = self.index_lock.lock().unwrap();
        if let Some(cached) = self.state.lock().unwrap().libretro_indexes.get(&repo) {
            return cached.clone();
        }
        let cache_name = format!("names-{repo}.txt");
        let url = format!(
            "https://api.github.com/repos/libretro-thumbnails/{repo}/git/trees/master:Named_Boxarts"
        );
        let text =
            self.cached_text_transform(&cache_name, Duration::from_secs(14 * 86_400), || {
                let Http::Ok(body) = http_get(&url, LIST_TIMEOUT_SECS) else {
                    return None;
                };
                let json: serde_json::Value = serde_json::from_slice(&body).ok()?;
                if json.get("truncated").and_then(|v| v.as_bool()) == Some(true) {
                    return None;
                }
                let entries = json.get("tree")?.as_array()?;
                let mut names = Vec::with_capacity(entries.len());
                for entry in entries {
                    if entry.get("type").and_then(|v| v.as_str()) != Some("blob") {
                        continue;
                    }
                    if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                        if let Some(stem) = path.strip_suffix(".png") {
                            names.push(stem);
                        }
                    }
                }
                (!names.is_empty()).then(|| names.join("\n"))
            });
        let idx = text.map(|t| Arc::new(ArtworkIndex::new(t.lines().map(str::to_owned).collect())));
        self.state
            .lock()
            .unwrap()
            .libretro_indexes
            .insert(repo, idx.clone());
        idx
    }

    fn nointro_title(&self, system: &str, crc: &str) -> Option<String> {
        if let Some(map) = self.state.lock().unwrap().nointro.get(system) {
            return map.get(crc).cloned();
        }
        let _guard = self.index_lock.lock().unwrap();
        if let Some(map) = self.state.lock().unwrap().nointro.get(system) {
            return map.get(crc).cloned();
        }
        let repo = system.replace(' ', "_");
        let url = format!(
            "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro/{}.dat",
            escape_path(system)
        );
        let map = self
            .cached_bytes(
                &format!("no-intro-{repo}.dat"),
                Duration::from_secs(30 * 86_400),
                &url,
            )
            .map(|b| matching::parse_nointro_dat(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        let arc = Arc::new(map);
        let title = arc.get(crc).cloned();
        self.state
            .lock()
            .unwrap()
            .nointro
            .insert(system.to_owned(), arc);
        title
    }

    fn arcade_title(&self, short: &str) -> Option<String> {
        let key = short.to_ascii_lowercase();
        if let Some(map) = self.state.lock().unwrap().arcade.clone() {
            return map.get(&key).cloned();
        }
        let _guard = self.index_lock.lock().unwrap();
        if let Some(map) = self.state.lock().unwrap().arcade.clone() {
            return map.get(&key).cloned();
        }
        let url = "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/fbneo-split/FBNeo%20-%20Arcade%20Games.dat";
        let map = self
            .cached_bytes("fbneo-titles.dat", Duration::from_secs(30 * 86_400), url)
            .map(|b| matching::parse_arcade_dat(&String::from_utf8_lossy(&b)))
            .unwrap_or_default();
        let arc = Arc::new(map);
        let title = arc.get(&key).cloned();
        self.state.lock().unwrap().arcade = Some(arc);
        title
    }

    fn cached_bytes(&self, filename: &str, max_age: Duration, url: &str) -> Option<Vec<u8>> {
        let path = self.cache_dir.join(filename);
        if is_fresh(&path, max_age) {
            if let Ok(bytes) = std::fs::read(&path) {
                return Some(bytes);
            }
        }
        if let Http::Ok(bytes) = http_get(url, LIST_TIMEOUT_SECS) {
            let _ = write_atomic(&path, &bytes);
            return Some(bytes);
        }
        // Stale cache is better than nothing when offline or rate-limited.
        std::fs::read(&path).ok()
    }

    fn cached_text_transform(
        &self,
        filename: &str,
        max_age: Duration,
        fetch: impl FnOnce() -> Option<String>,
    ) -> Option<String> {
        let path = self.cache_dir.join(filename);
        if is_fresh(&path, max_age) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                return Some(text);
            }
        }
        if let Some(text) = fetch() {
            let _ = write_atomic(&path, text.as_bytes());
            return Some(text);
        }
        std::fs::read_to_string(&path).ok()
    }
}

fn is_fresh(path: &Path, max_age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < max_age)
}

fn libretro_systems(console: Console) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut push = |s: &'static str| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    if let Some(primary) = crate::details::thumbnail_system(console) {
        push(primary);
    }
    for &sib in matching::sibling_systems(console) {
        if let Some(s) = crate::details::thumbnail_system(sib) {
            push(s);
        }
    }
    for &extra in matching::extra_libretro_systems(console) {
        push(extra);
    }
    out
}

fn ds_regions(code: &str) -> Vec<&'static str> {
    let own = match code.as_bytes().last() {
        Some(b'J') => "JA",
        Some(b'K') => "KO",
        Some(b'D') => "DE",
        Some(b'F') => "FR",
        Some(b'I') => "IT",
        Some(b'S') => "ES",
        Some(b'E') => "US",
        _ => "EN",
    };
    let mut out = vec![own];
    for r in ["US", "EN", "JA"] {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

fn fetch_libretro_box(repo: &str, name: &str) -> Result<Option<Img>, String> {
    let url = format!(
        "https://raw.githubusercontent.com/libretro-thumbnails/{repo}/master/Named_Boxarts/{}.png",
        escape_path(name)
    );
    fetch_image(&url, 3)
}

fn fetch_image(url: &str, hops: u8) -> Result<Option<Img>, String> {
    match http_get(url, TIMEOUT_SECS) {
        Http::Ok(bytes) => {
            if let Some(img) = gfx::decode_image(&bytes) {
                return Ok(Some(img));
            }
            // Libretro stores duplicate covers as git symlinks served as tiny text files.
            if hops > 0 {
                if let Some(target) = link_target(&bytes) {
                    if let Some(next) = resolve_relative(url, &target) {
                        return fetch_image(&next, hops - 1);
                    }
                }
            }
            Ok(None)
        }
        Http::NotFound => Ok(None),
        Http::Error(msg) => Err(msg),
    }
}

fn link_target(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() >= 1024 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?.trim();
    if text.is_empty() || text.contains('\n') || text.starts_with('/') || text.contains("://") {
        return None;
    }
    let ext = text.rsplit_once('.')?.1;
    if !matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg") {
        return None;
    }
    Some(text.to_owned())
}

fn resolve_relative(base: &str, rel: &str) -> Option<String> {
    let (prefix, _) = base.rsplit_once('/')?;
    let mut parts: Vec<&str> = prefix.split('/').collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.len() <= 3 {
                    return None;
                }
                parts.pop();
            }
            other => {
                let encoded = escape_path(other);
                let mut joined = parts.join("/");
                joined.push('/');
                joined.push_str(&encoded);
                // Simple relative targets in Libretro are a single sibling filename.
                if !rel.contains('/') || rel.ends_with(other) {
                    return Some(joined);
                }
                parts.push(other);
            }
        }
    }
    None
}

enum Http {
    Ok(Vec<u8>),
    NotFound,
    Error(String),
}

fn user_agent() -> String {
    format!(
        "DSlot/{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_REPOSITORY")
    )
}

fn http_get(url: &str, timeout: &str) -> Http {
    // Write body to a unique scratch buffer via stdout and capture status code on stderr via -w.
    let out = Command::new("curl")
        .args([
            "-sSL",
            "--connect-timeout",
            "10",
            "--max-time",
            timeout,
            "-A",
            &user_agent(),
            "-w",
            "\n%{http_code}",
            url,
        ])
        .output();
    let Ok(out) = out else {
        return Http::Error("curl is not available".into());
    };
    if !out.status.success() {
        return Http::Error("No network connection".into());
    }
    let bytes = out.stdout;
    let Some(nl) = bytes.iter().rposition(|&b| b == b'\n') else {
        return Http::Error("Invalid server response".into());
    };
    let code = std::str::from_utf8(&bytes[nl + 1..]).unwrap_or("").trim();
    match code {
        "200" => Http::Ok(bytes[..nl].to_vec()),
        "404" | "410" => Http::NotFound,
        "403" | "429" => Http::Error("Rate limited; try again later".into()),
        other => Http::Error(format!("Server returned HTTP {other}")),
    }
}

fn escape_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn save_image(
    game: &Game,
    folder: Folder,
    img: Img,
    flatten_white: bool,
    replace: bool,
) -> Result<(), String> {
    if !game.path.exists() {
        return Err("ROM file unavailable".into());
    }
    let dest = game.art(folder.name());
    if !replace && dest.is_file() {
        return Ok(());
    }
    let mut resized = img.fit_max_edge(MAX_EDGE);
    if flatten_white {
        resized.flatten_white();
    }
    let parent = dest.parent().ok_or("invalid artwork path")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", folder.name()))?;
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = parent.join(format!(".dslot-art-{}-{seq}.part", std::process::id()));
    let res = gfx::save_png(&tmp, resized.w as u32, resized.h as u32, &resized.px);
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("cannot save artwork: {e}"));
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("part-{seq}"));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn saves_resized_png_and_preserves_existing() {
        let dir = TempDir::new("art-save");
        let snes = dir.path().join("SFC");
        std::fs::create_dir_all(&snes).unwrap();
        let rom = snes.join("Chrono Trigger (USA).sfc");
        std::fs::write(&rom, b"rom").unwrap();
        let game = Game {
            console: Console::Snes,
            path: rom,
            stem: "Chrono Trigger (USA)".into(),
            code: String::new(),
            title: vec!["Chrono Trigger".into()],
            icon: None,
            cover: None,
            gone: false,
            group: String::new(),
            folder: false,
            sub: String::new(),
        };

        // 1024x512 transparent image -> scaled to 512x256, box flattened onto white.
        let mut src = Img::new(1024, 512);
        src.px[0..4].copy_from_slice(&[200, 100, 50, 0]);
        save_image(&game, Folder::Imgs, src.clone(), true, false).unwrap();
        let saved = gfx::load_png(&game.art("Imgs")).unwrap();
        assert_eq!((saved.w, saved.h), (512, 256));
        assert_eq!(saved.get(0, 0), [255, 255, 255, 255]);

        // Cart keeps alpha transparency.
        save_image(&game, Folder::Carts, src, false, false).unwrap();
        let cart = gfx::load_png(&game.art("Carts")).unwrap();
        assert_eq!((cart.w, cart.h), (512, 256));
        assert_eq!(cart.get(0, 0)[3], 0);

        // Existing file is never overwritten when replace = false.
        let mut solid = Img::new(64, 64);
        solid.px.fill(12);
        save_image(&game, Folder::Imgs, solid.clone(), true, false).unwrap();
        assert_eq!(gfx::load_png(&game.art("Imgs")).unwrap().w, 512);

        // With replace = true, it replaces.
        save_image(&game, Folder::Imgs, solid, true, true).unwrap();
        assert_eq!(gfx::load_png(&game.art("Imgs")).unwrap().w, 64);
    }

    #[test]
    fn parses_git_symlink_target() {
        assert_eq!(
            link_target(b"Super Mario World (USA).png\n").as_deref(),
            Some("Super Mario World (USA).png")
        );
        assert_eq!(link_target(b"https://example.com/a.png"), None);
        assert_eq!(link_target(b"/abs/path.png"), None);
        assert_eq!(
            resolve_relative(
                "https://raw.githubusercontent.com/libretro-thumbnails/SNES/master/Named_Boxarts/Old.png",
                "New Game (USA).png"
            )
            .as_deref(),
            Some("https://raw.githubusercontent.com/libretro-thumbnails/SNES/master/Named_Boxarts/New%20Game%20%28USA%29.png")
        );
    }

    /// Live network smoke test across Libretro, GameTDB, LaunchBox (cart + disc + UMD + G&W).
    #[test]
    #[ignore]
    fn live_fetch_across_sources() {
        let dir = TempDir::new("art-live");
        let cache = dir.path().join("cache");
        let svc = Service::new(&cache, vec![PathBuf::from("data/art")]);
        let cancel = AtomicBool::new(false);
        let mk = |sub: &str, file: &str, console: Console, code: &str| {
            let folder = dir.path().join(sub);
            std::fs::create_dir_all(&folder).unwrap();
            let path = folder.join(file);
            std::fs::write(&path, b"dummy").unwrap();
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            Game {
                console,
                path,
                stem,
                code: code.into(),
                title: vec!["Test".into()],
                icon: None,
                cover: None,
                gone: false,
                group: String::new(),
                folder: false,
                sub: String::new(),
            }
        };
        // SNES: Libretro box + LaunchBox cart
        let snes = mk("SFC", "Super Mario World (USA).sfc", Console::Snes, "");
        let o = svc.process_game(&snes, ArtKind::Both, false, &cancel);
        assert!(o.box_saved && o.cart_saved, "SNES: {o:?}");
        assert!(snes.art("Imgs").is_file() && snes.art("Carts").is_file());

        // PlayStation: Libretro box + LaunchBox disc -> Labels/
        let ps = mk("PS", "Crash Bandicoot (USA).chd", Console::PlayStation, "");
        let o = svc.process_game(&ps, ArtKind::Both, false, &cancel);
        assert!(o.box_saved && o.cart_saved, "PS: {o:?}");
        assert!(ps.art("Imgs").is_file() && ps.art("Labels").is_file());

        // PSP: Libretro box + LaunchBox UMD -> Carts/
        let psp = mk("PSP", "Metal Gear Acid (USA).iso", Console::Psp, "");
        let o = svc.process_game(&psp, ArtKind::Both, false, &cancel);
        assert!(o.box_saved && o.cart_saved, "PSP: {o:?}");
        assert!(psp.art("Imgs").is_file() && psp.art("Carts").is_file());

        // DS: GameTDB label -> Labels/ + cover -> Imgs/
        let ds = mk("NDS", "Mario Kart DS (USA).nds", Console::Ds, "AMCE");
        let o = svc.process_game(&ds, ArtKind::Both, false, &cancel);
        assert!(o.box_saved && o.cart_saved, "DS: {o:?}");
        assert!(ds.art("Imgs").is_file() && ds.art("Labels").is_file());

        // Game & Watch: LaunchBox box fallback for a game Libretro lacks!
        let gw = mk("GW", "gnw_zelda.zip", Console::GameWatch, "");
        let o = svc.process_game(&gw, ArtKind::BoxOnly, false, &cancel);
        assert!(o.box_saved, "G&W: {o:?}");
        assert!(gw.art("Imgs").is_file());
    }

    #[test]
    fn clear_caches_drops_in_memory_index_maps() {
        let svc = Service::new(Path::new("/tmp/dslot-test-art"), Vec::new());
        {
            let mut st = svc.state.lock().unwrap();
            st.libretro_indexes.insert("Nintendo_Game_Boy".into(), None);
            st.nointro
                .insert("Game Boy".into(), Arc::new(HashMap::new()));
            st.arcade = Some(Arc::new(HashMap::new()));
            st.slot_labels = Some(Arc::new(HashMap::new()));
            st.launchbox.insert("SNES".into(), None);
        }
        svc.clear_caches();
        let st = svc.state.lock().unwrap();
        assert!(st.libretro_indexes.is_empty());
        assert!(st.nointro.is_empty());
        assert!(st.arcade.is_none());
        assert!(st.slot_labels.is_none());
        assert!(st.launchbox.is_empty());
    }
}
