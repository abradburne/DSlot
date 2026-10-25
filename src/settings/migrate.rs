//! Reads what DSlot kept before settings.toml, once: `settings.ini` (DSlot 0.2: the menu's
//! choices, `key=value` lines) and `launch.conf` (written by DSperate's launcher, then by DSlot
//! 0.3 before settings.toml: `<what> = <options>` and `<game> : <emulator>` lines). Options an
//! emulator declares become its settings (`--hires3d 2` is DSperate's 3D resolution, 2); any
//! other words are kept as its extra arguments at the same level. The old files are then
//! renamed `*.old`.

use super::{console_chain, console_scope, def, game_chain, Scope, Settings};
use crate::emulator::{Emulator, Emulators};
use crate::rom::{Console, Game};
use std::path::PathBuf;
use toml::Value;

/// Where the older files are.
pub struct Legacy {
    pub settings_ini: Option<PathBuf>,
    pub launch_conf: Option<PathBuf>,
}

impl Legacy {
    /// No older files at all (tests).
    #[cfg(test)]
    pub fn none() -> Legacy {
        Legacy {
            settings_ini: None,
            launch_conf: None,
        }
    }

    fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.settings_ini.iter().chain(&self.launch_conf)
    }

    /// Renames the files read to `<name>.old`, so they are not read again.
    pub fn retire(&self) {
        for file in self.files().filter(|f| f.exists()) {
            let mut old = file.clone().into_os_string();
            old.push(".old");
            if let Err(e) = std::fs::rename(file, &old) {
                eprintln!("dslot: cannot rename {}: {e}", file.display());
            }
        }
    }
}

/// Reads the older files into `settings`. Returns whether there were any.
pub fn migrate(
    settings: &mut Settings,
    legacy: &Legacy,
    games: &[Game],
    emulators: &Emulators,
) -> bool {
    let read = |file: &Option<PathBuf>| file.as_ref().and_then(|f| std::fs::read_to_string(f).ok());
    let (ini, conf) = (read(&legacy.settings_ini), read(&legacy.launch_conf));
    if let Some(ini) = &ini {
        settings_ini(settings, ini);
    }
    if let Some(conf) = &conf {
        launch_conf(settings, conf, games, emulators);
    }
    if ini.is_some() || conf.is_some() {
        let names: Vec<String> = legacy
            .files()
            .filter(|f| f.exists())
            .map(|f| f.display().to_string())
            .collect();
        eprintln!("dslot: settings read from {}", names.join(" and "));
        return true;
    }
    false
}

fn settings_ini(settings: &mut Settings, text: &str) {
    for (key, value) in text.lines().filter_map(|l| l.split_once('=')) {
        let (key, value) = (key.trim(), value.trim());
        match key {
            "case" | "view" | "sort" => settings.set(Scope::System, key, Value::from(value)),
            "show" => settings.set(Scope::System, def::SHOW, Value::from(value)),
            "sound" => settings.set(Scope::System, def::SOUNDS, Value::Boolean(value != "off")),
            "page" => settings.page = Console::from_key(value).unwrap_or(settings.page),
            _ => {
                let console = key.strip_prefix("emulator.").and_then(Console::from_key);
                if let Some(console) = console.filter(|_| !value.is_empty()) {
                    settings.set(console_scope(console), def::EMULATOR, Value::from(value));
                }
            }
        }
    }
}

/// A launch.conf line: what it is for, and its options or emulator.
enum Line<'a> {
    Options(&'a str, Vec<String>),
    Emulator(&'a str, &'a str),
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        // Whichever comes first: options may hold a ':', and a FAT card's file names can't.
        let Some(at) = line.find(['=', ':']) else {
            continue;
        };
        let (key, value) = (line[..at].trim(), line[at + 1..].trim());
        if line[at..].starts_with('=') {
            out.push(Line::Options(
                key,
                value.split_whitespace().map(str::to_owned).collect(),
            ));
        } else {
            out.push(Line::Emulator(key, value));
        }
    }
    out
}

fn launch_conf(settings: &mut Settings, text: &str, games: &[Game], emulators: &Emulators) {
    let lines = lines(text);
    let named = |stem: &str| -> Vec<&Game> { games.iter().filter(|g| g.stem == stem).collect() };
    // Games' emulators first: the options for a game go to the emulator it starts in.
    for line in &lines {
        if let Line::Emulator(stem, id) = line {
            for game in named(stem) {
                settings.set(Scope::Game(game.key()), def::EMULATOR, Value::from(*id));
            }
        }
    }
    for line in &lines {
        let Line::Options(key, words) = line else {
            continue;
        };
        if let Some(emulator) = emulators.get(key) {
            // Every game started in it: each console it plays.
            for console in Console::ALL
                .into_iter()
                .filter(|&c| emulator.plays(c.folder()))
            {
                place(settings, console_scope(console), emulator, console, words);
            }
        } else if let Some(console) = Console::from_folder(key) {
            let chosen = settings
                .emulator(&console_chain(console))
                .map(str::to_owned);
            match emulators.choose(console.folder(), chosen.as_deref()) {
                Some(emulator) => place(settings, console_scope(console), emulator, console, words),
                None => eprintln!(
                    "dslot: launch.conf: no emulator for {key} games; \"{}\" left out",
                    words.join(" ")
                ),
            }
        } else {
            let games = named(key);
            if games.is_empty() {
                eprintln!("dslot: launch.conf: no game {key}; its line left out");
            }
            for game in games {
                let chosen = settings.emulator(&game_chain(game)).map(str::to_owned);
                if let Some(emulator) = emulators.choose(game.console.folder(), chosen.as_deref()) {
                    place(
                        settings,
                        Scope::Game(game.key()),
                        emulator,
                        game.console,
                        words,
                    );
                }
            }
        }
    }
}

/// Sets `words` at `scope`: the emulator's declared options as its settings, anything else as
/// its extra arguments.
fn place(
    settings: &mut Settings,
    scope: Scope,
    emulator: &Emulator,
    console: Console,
    words: &[String],
) {
    let mut rest: Vec<String> = Vec::new();
    let mut at = 0;
    'words: while at < words.len() {
        for option in emulator.options_for(console.folder()) {
            if let Some((taken, value)) = option.recognise(&words[at..]) {
                settings.set(scope.clone(), &option.def.key, value);
                at += taken;
                continue 'words;
            }
        }
        rest.push(words[at].clone());
        at += 1;
    }
    if !rest.is_empty() {
        let key = format!("{}.{}", emulator.id, def::ARGS);
        let before = settings
            .get(&scope, &key)
            .and_then(Value::as_str)
            .map(str::to_owned);
        let args = before
            .into_iter()
            .chain([rest.join(" ")])
            .collect::<Vec<_>>()
            .join(" ");
        settings.set(scope, &key, Value::from(args));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    const DSPERATE: &str = r#"
        name = "DSperate"
        priority = 100
        [systems]
        NDS = ["/bin/sh", "{options}", "{rom}"]
        GBA = ["/bin/sh", "--gba", "{options}", "{rom}"]
        [[options]]
        key = "hires3d"
        label = "3D resolution"
        systems = ["NDS"]
        values = [1, 2, 3, 4]
        args = ["--hires3d", "{value}"]
        [[options]]
        key = "aa"
        label = "3D anti-aliasing"
        kind = "toggle"
        args = ["--aa"]
        off = ["--no-aa"]
    "#;

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
    fn from_older_files() {
        let dir = TempDir::new("migrate");
        std::fs::create_dir_all(dir.path().join("emulators")).unwrap();
        std::fs::write(dir.path().join("emulators/dsperate.toml"), DSPERATE).unwrap();
        let emulators = Emulators::load(&[dir.path().join("emulators")]);
        let ini = dir.path().join("settings.ini");
        let conf = dir.path().join("launch.conf");
        std::fs::write(
            &ini,
            "case=black\nview=grid\nshow=favourites\nsound=off\npage=gba\nemulator.gba=mgba\n",
        )
        .unwrap();
        std::fs::write(
            &conf,
            "# DSperate's launcher's example\nNDS = --hires3d 2 --frameskip 1\ndsperate = --aa\n\
             Golden Sun = --hires3d 1\nAdvance Wars : dsperate\nAdvance Wars = --speed 150\n",
        )
        .unwrap();
        let games = [
            game(Console::Ds, "Golden Sun"),
            game(Console::Gba, "Advance Wars"),
        ];
        let legacy = Legacy {
            settings_ini: Some(ini.clone()),
            launch_conf: Some(conf.clone()),
        };
        let file = dir.path().join("settings.toml");
        let s = Settings::load(file.clone(), &legacy, &games, &emulators);

        assert_eq!(
            (
                s.case(Console::Ds),
                s.view(Console::Gba),
                s.show(Console::Ds)
            ),
            (
                super::super::Case::Black,
                super::super::View::Grid,
                super::super::Show::Favourites
            )
        );
        assert!(!s.sounds());
        assert_eq!(s.page, Console::Gba);
        let ds = console_scope(Console::Ds);
        let int = |scope: &Scope, key: &str| s.get(scope, key).and_then(Value::as_integer);
        let text =
            |scope: &Scope, key: &str| s.get(scope, key).and_then(Value::as_str).map(str::to_owned);
        // `--hires3d 2` is the option; `--frameskip 1` is not declared, so it stays words.
        assert_eq!(int(&ds, "dsperate.hires3d"), Some(2));
        assert_eq!(text(&ds, "dsperate.args").as_deref(), Some("--frameskip 1"));
        // The emulator's own line goes to each console it plays.
        assert_eq!(s.get(&ds, "dsperate.aa"), Some(&Value::Boolean(true)));
        assert_eq!(
            s.get(&console_scope(Console::Gba), "dsperate.aa"),
            Some(&Value::Boolean(true))
        );
        // A game's line and emulator, at the game.
        assert_eq!(
            int(&Scope::Game("NDS/Golden Sun".into()), "dsperate.hires3d"),
            Some(1)
        );
        let aw = Scope::Game("GBA/Advance Wars".into());
        assert_eq!(text(&aw, "emulator").as_deref(), Some("dsperate"));
        assert_eq!(text(&aw, "dsperate.args").as_deref(), Some("--speed 150"));
        // GBA's emulator from settings.ini, though mGBA isn't installed here.
        assert_eq!(
            text(&console_scope(Console::Gba), "emulator").as_deref(),
            Some("mgba")
        );

        // Read once: the old files are put aside, and settings.toml is what is read next time.
        assert!(!ini.exists() && !conf.exists() && dir.path().join("launch.conf.old").exists());
        let again = Settings::load(file, &legacy, &games, &emulators);
        assert_eq!(
            again
                .get(&ds, "dsperate.hires3d")
                .and_then(Value::as_integer),
            Some(2)
        );
    }
}
