//! DSlot's settings, at three levels: the **system** (DSlot and the device as a whole), a
//! **console** (all its games) and a **game**. A value set for a game wins over its console's,
//! which wins over the system's; one set nowhere is the setting's default. What can be set at
//! which level is the setting's (`def`). The emulator's options live here too, under its id
//! (`dsperate.hires3d`), so the menu shows them and launching reads them from the same place.
//!
//! nibkit's layered store keeps them, in one file, `settings.toml` (platform::dirs), with the
//! shelf's page in its `[state]` table:
//!
//! ```toml
//! [state]
//! page = "gba"
//!
//! [system]
//! view = "grid"
//!
//! [console.NDS]
//! emulator = "dsperate"
//!
//! [console.NDS.dsperate]
//! hires3d = 2
//!
//! [game."NDS/Golden Sun - Dark Dawn (USA)".dsperate]
//! hires3d = 1
//! ```
//!
//! DSlot 0.2's `settings.ini` and 0.3's first `launch.conf` are read into it once (`migrate`).

pub mod def;
mod migrate;

pub use def::{Case, Repeat, Saver, SaverLook, Show, SnesCart, Sort, View};
pub use migrate::Legacy;
pub use nibkit::settings::Scope;

use crate::emulator::Emulators;
use crate::rom::{Console, Game};
use def::Def;
use std::path::PathBuf;
use toml::Value;

/// The levels a game's settings are looked up at, most particular first.
pub fn game_chain(game: &Game) -> [Scope; 3] {
    Scope::chain(game.console.folder(), &game.stem)
}

/// The levels a console's settings are looked up at.
pub fn console_chain(console: Console) -> [Scope; 2] {
    Scope::console_chain(console.folder())
}

/// A console's level.
pub fn console_scope(console: Console) -> Scope {
    Scope::Console(console.folder().to_owned())
}

pub struct Settings {
    store: nibkit::settings::Settings,
    /// The console whose page the shelf is on.
    pub page: Console,
}

/// The file's first lines.
const HEADER: &str = "\
# DSlot's settings: [system], then [console.<NDS|GBA|GB|GBC|SFC|FC|FDS|MD|SMS|SG1000|GG|PCE|PCECD|PS|LYNX|NGP|WS|GW>], then [game.\"<console>/<ROM name>\"],
# each winning over the one before. An emulator's options are under its id, [console.NDS.dsperate].
# DSlot writes this file whenever a setting changes in its menu, and reads it again after each game
# (which may change its own options here); edit it while DSlot is not running.
";

impl Settings {
    /// Reads `file`; if there is none yet, what older versions of DSlot kept instead.
    pub fn load(file: PathBuf, legacy: &Legacy, games: &[Game], emulators: &Emulators) -> Settings {
        let existed = file.exists();
        let mut store = nibkit::settings::Settings::load(file);
        store.header = HEADER.to_owned();
        let page = store
            .state
            .get("page")
            .and_then(Value::as_str)
            .and_then(Console::from_key);
        let mut settings = Settings {
            store,
            page: page.unwrap_or(Console::Ds),
        };
        if !existed && migrate::migrate(&mut settings, legacy, games, emulators) {
            settings.save();
            legacy.retire();
        }
        settings
    }

    /// Reads the file again: a game may have changed its own options in it. The page shown is
    /// kept as it is.
    pub fn reload(&mut self) {
        let mut store = nibkit::settings::Settings::load(self.store.file());
        store.header = HEADER.to_owned();
        self.store = store;
    }

    pub fn save(&mut self) {
        self.store
            .state
            .insert("page".into(), Value::from(self.page.key()));
        if let Err(e) = self.store.save() {
            eprintln!("dslot: cannot write {}: {e}", self.store.file().display());
        }
    }

    /// The value set at exactly this level.
    pub fn get(&self, scope: &Scope, key: &str) -> Option<&Value> {
        self.store.get(scope, key)
    }

    pub fn set(&mut self, scope: Scope, key: &str, value: Value) {
        self.store.set(scope, key, value)
    }

    /// Lets this level inherit `key` again.
    pub fn reset(&mut self, scope: &Scope, key: &str) {
        self.store.reset(scope, key)
    }

    /// The value of `key` seen from the first level of `chain`, and which level sets it.
    pub fn resolve<'a, 's>(
        &'a self,
        chain: &'s [Scope],
        key: &str,
    ) -> Option<(&'a Value, &'s Scope)> {
        self.store.resolve(chain, key)
    }

    /// `def`'s value seen from `chain`: as set at a level it can be set at, if it is a value it
    /// takes; else its default.
    pub fn value(&self, chain: &[Scope], def: &Def) -> Option<Value> {
        self.store.value(chain, def)
    }

    /// The level along `chain` `def`'s value comes from; `None` for its default.
    pub fn source<'s>(&self, chain: &'s [Scope], def: &Def) -> Option<&'s Scope> {
        self.store.source(chain, def)
    }

    /// Whether `def` is set at a level along `chain` it can be set at, to a value it takes.
    pub fn has(&self, chain: &[Scope], def: &Def) -> bool {
        self.store.source(chain, def).is_some()
    }

    fn choice<T>(&self, chain: &[Scope], key: &str, parse: fn(&str) -> Option<T>) -> T {
        let def = def::builtin(key).unwrap_or_else(|| unreachable!("{key} is a built-in setting"));
        let value = self.value(chain, &def);
        value
            .as_ref()
            .and_then(Value::as_str)
            .and_then(parse)
            .unwrap_or_else(|| {
                let default = def
                    .default
                    .as_ref()
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                parse(default)
                    .unwrap_or_else(|| unreachable!("{key}'s default is one of its values"))
            })
    }

    /// How `console`'s page is shown, sorted and filtered.
    pub fn view(&self, console: Console) -> View {
        self.choice(&console_chain(console), def::VIEW, View::parse)
    }

    pub fn sort(&self, console: Console) -> Sort {
        self.choice(&console_chain(console), def::SORT, Sort::parse)
    }

    pub fn show(&self, console: Console) -> Show {
        self.choice(&console_chain(console), def::SHOW, Show::parse)
    }

    pub fn case(&self, console: Console) -> Case {
        self.choice(&console_chain(console), def::CASE, Case::parse)
    }

    /// What `console`'s artwork looks like besides its own pictures: its boxes' case, and (on
    /// the SNES page) its carts' style.
    pub fn look(&self, console: Console) -> crate::art::Looks {
        crate::art::Looks {
            case: self.case(console),
            snes: self.choice(
                &console_chain(Console::Snes),
                def::SNES_CART,
                SnesCart::parse,
            ),
        }
    }

    pub fn repeat(&self) -> Repeat {
        self.choice(&[Scope::System], def::REPEAT, Repeat::parse)
    }

    pub fn saver(&self) -> Saver {
        self.choice(&[Scope::System], def::SAVER, Saver::parse)
    }

    pub fn saver_look(&self) -> SaverLook {
        self.choice(&[Scope::System], def::SAVER_LOOK, SaverLook::parse)
    }

    /// Whether a console with no games has a page all the same.
    pub fn show_empty(&self) -> bool {
        let def = def::builtin(def::SHOW_EMPTY)
            .unwrap_or_else(|| unreachable!("show_empty_systems is a built-in setting"));
        self.value(&[Scope::System], &def)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn sounds(&self) -> bool {
        let def = def::builtin(def::SOUNDS)
            .unwrap_or_else(|| unreachable!("sounds is a built-in setting"));
        self.value(&[Scope::System], &def)
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }

    /// RetroAchievements: hardcore was chosen (softcore by default). Whether it is offered at
    /// all is the achievements module's to say (`Ra::env`).
    pub fn hardcore_chosen(&self) -> bool {
        let def = def::builtin(def::ACHIEVEMENTS)
            .unwrap_or_else(|| unreachable!("achievements is a built-in setting"));
        self.value(&[Scope::System], &def)
            .and_then(|v| v.as_str().and_then(def::Achievements::parse))
            == Some(def::Achievements::Hardcore)
    }

    /// The emulator chosen along `chain`, by id (whether or not it is installed).
    pub fn emulator<'a>(&'a self, chain: &[Scope]) -> Option<&'a str> {
        self.resolve(chain, def::EMULATOR)
            .and_then(|(v, _)| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;

    fn game(console: Console, stem: &str) -> Game {
        let path = PathBuf::from(format!("/r/{}/{stem}.rom", console.folder()));
        Game {
            console,
            path,
            stem: stem.into(),
            code: String::new(),
            title: vec![stem.into()],
            icon: None,
            cover: None,
            gone: false,
        }
    }

    #[test]
    fn levels() {
        let dir = TempDir::new("settings-levels");
        let mut s = Settings::load(
            dir.path().join("settings.toml"),
            &Legacy::none(),
            &[],
            &Emulators::default(),
        );
        let gs = game(Console::Ds, "Golden Sun");
        let chain = game_chain(&gs);
        assert_eq!(s.view(Console::Ds), View::Shelf);
        assert_eq!(s.resolve(&chain, "dsperate.hires3d"), None);

        s.set(Scope::System, def::VIEW, Value::from("grid"));
        s.set(console_scope(Console::Gba), def::VIEW, Value::from("shelf"));
        assert_eq!(
            (s.view(Console::Ds), s.view(Console::Gba)),
            (View::Grid, View::Shelf)
        );

        s.set(
            console_scope(Console::Ds),
            "dsperate.hires3d",
            Value::Integer(2),
        );
        let (v, from) = s.resolve(&chain, "dsperate.hires3d").unwrap();
        assert_eq!(
            (v.as_integer(), from),
            (Some(2), &console_scope(Console::Ds))
        );
        s.set(Scope::Game(gs.key()), "dsperate.hires3d", Value::Integer(1));
        assert_eq!(
            s.resolve(&chain, "dsperate.hires3d")
                .unwrap()
                .0
                .as_integer(),
            Some(1)
        );
        s.reset(&Scope::Game(gs.key()), "dsperate.hires3d");
        assert_eq!(
            s.resolve(&chain, "dsperate.hires3d").unwrap().1,
            &console_scope(Console::Ds)
        );

        // A value the setting doesn't take is as good as none.
        s.set(Scope::System, def::SORT, Value::from("colour"));
        assert_eq!(s.sort(Console::Ds), Sort::Name);
    }

    #[test]
    fn round_trip() {
        let dir = TempDir::new("settings-file");
        let file = dir.path().join("config/dslot/settings.toml");
        let mut s = Settings::load(file.clone(), &Legacy::none(), &[], &Emulators::default());
        let gs = game(Console::Ds, "Golden Sun - Dark Dawn (USA)");
        s.page = Console::Gba;
        s.set(Scope::System, def::SOUNDS, Value::Boolean(false));
        s.set(
            console_scope(Console::Ds),
            def::EMULATOR,
            Value::from("dsperate"),
        );
        s.set(
            console_scope(Console::Ds),
            "dsperate.hires3d",
            Value::Integer(2),
        );
        s.set(
            Scope::Game(gs.key()),
            "dsperate.args",
            Value::from("--speed 150"),
        );
        s.save();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# DSlot's settings"));
        assert!(text.contains("[console.NDS.dsperate]"), "{text}");

        let s = Settings::load(file, &Legacy::none(), &[], &Emulators::default());
        assert_eq!(s.page, Console::Gba);
        assert!(!s.sounds());
        assert_eq!(s.emulator(&game_chain(&gs)), Some("dsperate"));
        let args = s.get(&Scope::Game(gs.key()), "dsperate.args");
        assert_eq!(args.and_then(Value::as_str), Some("--speed 150"));
    }

    #[test]
    fn changed_by_a_game() {
        let dir = TempDir::new("settings-reload");
        let file = dir.path().join("settings.toml");
        let mut s = Settings::load(file.clone(), &Legacy::none(), &[], &Emulators::default());
        s.page = Console::Gb;
        s.set(Scope::System, def::SOUNDS, Value::Boolean(false));
        s.save();

        // The emulator sets an option of its own from its menu while the game runs.
        let mut theirs = nibkit::settings::Settings::load(&file);
        theirs.set(
            Scope::Console("GB".into()),
            "inkdot.palette",
            Value::from("grey"),
        );
        theirs.save().unwrap();

        s.reload();
        let chain = console_chain(Console::Gb);
        let (v, _) = s.resolve(&chain, "inkdot.palette").unwrap();
        assert_eq!(v.as_str(), Some("grey"));
        assert!(!s.sounds());
        // DSlot's next write keeps it, under DSlot's own header.
        s.save();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# DSlot's settings"), "{text}");
        assert!(text.contains("[console.GB.inkdot]"), "{text}");
        assert_eq!(s.page, Console::Gb);
    }

    #[test]
    fn hand_edited() {
        let dir = TempDir::new("settings-edited");
        let file = dir.path().join("settings.toml");
        fs::write(
            &file,
            "[system]\nview = \"wall\"\n[console.SNES]\nview = \"grid\"\n[state]\npage = \"gb\"\n",
        )
        .unwrap();
        let s = Settings::load(file.clone(), &Legacy::none(), &[], &Emulators::default());
        assert_eq!((s.view(Console::Ds), s.page), (View::Shelf, Console::Gb));
        // Not TOML at all: the defaults.
        fs::write(&file, "view = = grid").unwrap();
        let s = Settings::load(file, &Legacy::none(), &[], &Emulators::default());
        assert_eq!(s.view(Console::Ds), View::Shelf);
    }
}
