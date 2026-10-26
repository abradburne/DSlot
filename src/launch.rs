//! Starting a game: which emulator, with what options, and running it until it exits; and
//! the actions on a game's files the menu offers (deleting its saves, an emulator's own).
//!
//! The emulator is the game's own choice in the settings, else its console's, else the
//! console's default (emulator.rs), whichever is installed. Its options are the ones it
//! declares, each as set for the game, its console or the system (settings), followed by the
//! extra words set at each level, the system's first.

use crate::emulator::{Action, Emulator, Emulators, Seed};
use crate::rom::Game;
use crate::settings::{def, game_chain, Settings};
use nibkit::dslot;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The emulator `game` starts in.
pub fn emulator_for<'a>(
    emulators: &'a Emulators,
    settings: &Settings,
    game: &Game,
) -> Option<&'a Emulator> {
    // One that plays the game: installed for its system, and listing it where it lists the
    // games it plays (`[games]`, nibkit::dslot).
    let installed = |id: &&str| {
        emulators
            .get(id)
            .is_some_and(|e| e.takes(game.console.folder(), &game.stem))
    };
    let chain = game_chain(game);
    let chosen = chain
        .iter()
        .filter_map(|scope| settings.get(scope, def::EMULATOR)?.as_str())
        .find(installed);
    emulators.choose_for(game.console.folder(), &game.stem, chosen)
}

/// The words `{options}` stands for when `game` starts in `emulator`.
pub fn options(settings: &Settings, emulator: &Emulator, game: &Game) -> Vec<String> {
    let chain = game_chain(game);
    let mut words: Vec<String> = Vec::new();
    for option in emulator.options_for(game.console.folder()) {
        // Left unset, it isn't passed: the emulator's own setting holds.
        if settings.has(&chain, &option.def) {
            if let Some(value) = settings.value(&chain, &option.def) {
                words.extend(option.words(&value));
            }
        }
    }
    let args = format!("{}.{}", emulator.id, def::ARGS);
    for scope in chain.iter().rev() {
        if let Some(extra) = settings.get(scope, &args).and_then(toml::Value::as_str) {
            words.extend(extra.split_whitespace().map(str::to_owned));
        }
    }
    words
}

/// A game's saves folder under the platform's saves folder: `<root>/<console>/<ROM name>`.
pub fn saves_dir(root: &Path, game: &Game) -> PathBuf {
    dslot::saves_dir(root, game.console.folder(), &game.stem)
}

/// A word of a command or an action with its placeholders filled in.
fn fill(word: &str, game: &Game, saves: Option<&Path>) -> String {
    let p = dslot::Placeholders {
        rom: &game.path,
        name: &game.stem,
        system: game.console.folder(),
        saves,
    };
    dslot::fill(word, &p)
}

/// Whether `action` can be done for `game` here: it names `{saves}` only where there is a
/// saves folder.
pub fn can_do(action: &Action, saves: Option<&Path>) -> bool {
    saves.is_some() || !action.delete.iter().any(|p| p.contains("{saves}"))
}

/// Deletes the files `action` names for `game`. Returns how many went.
pub fn do_action(action: &Action, game: &Game, saves: Option<&Path>) -> usize {
    let saves = saves.map(|root| saves_dir(root, game));
    let mut gone = 0;
    for pattern in &action.delete {
        let path = PathBuf::from(fill(pattern, game, saves.as_deref()));
        let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
        else {
            continue;
        };
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let file = entry.file_name();
            if entry.path().is_file() && file.to_str().is_some_and(|f| dslot::matches(name, f)) {
                match std::fs::remove_file(entry.path()) {
                    Ok(()) => gone += 1,
                    Err(e) => eprintln!("dslot: cannot delete {}: {e}", entry.path().display()),
                }
            }
        }
    }
    eprintln!(
        "dslot: {}: {} for {}, {gone} files",
        action.label,
        game.stem,
        game.console.label()
    );
    gone
}

/// Deletes `game`'s saves folder and everything in it.
pub fn delete_saves(root: &Path, game: &Game) -> std::io::Result<()> {
    let dir = saves_dir(root, game);
    // Only ever a game's own folder, two levels under the saves folder ("..", the name of a
    // file called "...nds", would be the console's folder).
    let own = !matches!(game.stem.as_str(), "" | "." | "..") && !game.stem.contains('/');
    if !own || dir.parent().and_then(Path::parent) != Some(root) {
        return Err(std::io::Error::other(format!(
            "not a game's saves folder: {}",
            dir.display()
        )));
    }
    eprintln!("dslot: deleting {}", dir.display());
    match std::fs::remove_dir_all(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// A game, ready to start.
#[derive(Debug, PartialEq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// The game's saves folder, when the command names one; made before the game starts.
    pub saves: Option<PathBuf>,
    /// Where it runs: an app's own folder; else wherever DSlot is.
    pub dir: Option<PathBuf>,
    pub seed: Vec<Seed>,
    pub quit_on_output: Option<String>,
}

impl Launch {
    /// `emulator`'s command for `game`, its placeholders filled in. `saves` is the platform's
    /// saves folder, if DSlot decides where saves go there.
    pub fn new(
        emulator: &Emulator,
        game: &Game,
        options: &[String],
        saves: Option<&Path>,
    ) -> Result<Launch, String> {
        let template = emulator.command(game.console.folder()).ok_or_else(|| {
            format!(
                "{} does not play {} games",
                emulator.name,
                game.console.label()
            )
        })?;
        let uses_saves = template.iter().any(|w| w.contains("{saves}"));
        let saves = match (uses_saves, saves) {
            (false, _) => None,
            (true, Some(root)) => Some(saves_dir(root, game)),
            (true, None) => {
                return Err(format!(
                    "{}: no saves folder on this system for {{saves}}",
                    emulator.id
                ))
            }
        };
        let mut words = template.iter().flat_map(|word| match word.as_str() {
            "{options}" => options.to_vec(),
            word => vec![fill(word, game, saves.as_deref())],
        });
        let program = words.next().unwrap_or_default();
        Ok(Launch {
            program,
            args: words.collect(),
            env: emulator.env.clone(),
            saves,
            dir: None,
            seed: emulator.seed.clone(),
            quit_on_output: emulator.quit_on_output.clone(),
        })
    }

    /// An app from `Roms/APPS`: its command, in its folder, with its environment.
    pub fn app(found: &nibkit::dslot::Found) -> Result<Launch, String> {
        let mut words = found.app.command_in(&found.folder).into_iter();
        let program = words
            .next()
            .ok_or_else(|| format!("{}: an empty command", found.id))?;
        Ok(Launch {
            program,
            args: words.collect(),
            env: found.app.env.clone().into_iter().collect(),
            saves: None,
            dir: Some(found.folder.clone()),
            seed: Vec::new(),
            quit_on_output: None,
        })
    }

    /// Runs the game and waits for it to exit, or until `put_away` says it must go (the power
    /// menu was asked for): it is then asked to save its auto-resume state and quit (SIGTERM),
    /// and killed if it hasn't within [`QUIT_GRACE`].
    pub fn run(&self, put_away: impl Fn() -> bool) -> std::io::Result<Ended> {
        put_seeds(&self.seed);
        if let Some(dir) = &self.saves {
            // Without its saves folder (the card not mounted, say) the game still starts.
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("dslot: no saves folder {}: {e}", dir.display());
            }
        }
        let mut command = Command::new(&self.program);
        if let Some(dir) = &self.dir {
            command.current_dir(dir);
        }
        command
            .args(&self.args)
            .envs(self.env.iter().map(|(k, v)| (k, v)));
        eprintln!("dslot: starting {} {}", self.program, self.args.join(" "));
        let (child, tail) = match &self.quit_on_output {
            None => spawn_tailed(command)?,
            Some(cue) => spawn_watched(command, cue.clone())?,
        };
        wait(child, &tail, put_away)
    }
}

/// How a game ended.
#[derive(Debug)]
pub struct Ended {
    pub status: ExitStatus,
    /// It was put away for the power menu, and resumes when started again.
    pub put_away: bool,
    /// The last lines it wrote to stderr (at most [`TAIL`]), for saying why it came straight
    /// back. They went to DSlot's own stderr too, as they came.
    pub stderr: Vec<String>,
}

/// How many of a game's last stderr lines are kept.
pub const TAIL: usize = 20;

/// The last lines a game wrote, kept as they are forwarded to DSlot's stderr.
#[derive(Debug, Default)]
struct Tail {
    lines: Mutex<VecDeque<String>>,
    /// The game's output has ended (the pipe is closed).
    done: AtomicBool,
}

impl Tail {
    fn push(&self, line: String) {
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        if lines.len() == TAIL {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// The lines kept, once the game's output has ended (or shortly after the game has).
    fn take(&self) -> Vec<String> {
        let deadline = std::time::Instant::now() + TAIL_GRACE;
        while !self.done.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
        let lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        lines.iter().cloned().collect()
    }
}

/// How long after a game has exited its last lines are waited for: its output is read on a
/// thread, which may be a step behind (and a helper it left running may keep the pipe open).
const TAIL_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

/// Forwards each line of `log` to DSlot's stderr as it comes, keeping the last [`TAIL`]; and,
/// when `cue` says so, asks the game (`pid`) to quit on the line that contains it.
fn tee(log: impl Read + Send + 'static, cue: Option<(String, libc::pid_t)>) -> Arc<Tail> {
    let tail = Arc::new(Tail::default());
    let kept = Arc::clone(&tail);
    // Not joined: helpers the emulator leaves running keep the pipe open after it is gone.
    std::thread::spawn(move || {
        for line in BufReader::new(log).split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&line).into_owned();
            eprintln!("{line}");
            if let Some((cue, pid)) = &cue {
                if line.contains(cue) {
                    // SIGTERM, not SIGKILL: the stock DS emulator restores the GPU governor on
                    // the way out.
                    // SAFETY: kill(2) on our child's pid, not yet waited for while it writes
                    // to us.
                    unsafe { libc::kill(*pid, libc::SIGTERM) };
                }
            }
            kept.push(line);
        }
        kept.done.store(true, Ordering::Release);
    });
    tail
}

/// Starts a game with its stderr read by DSlot: forwarded to DSlot's own, and its last lines
/// kept.
fn spawn_tailed(mut command: Command) -> std::io::Result<(Child, Arc<Tail>)> {
    let mut child = command.stderr(Stdio::piped()).spawn()?;
    let Some(err) = child.stderr.take() else {
        return Err(std::io::Error::other("no pipe to the game's stderr"));
    };
    Ok((child, tee(err, None)))
}

/// How long a game asked to quit has to save its state before it is killed.
pub const QUIT_GRACE: std::time::Duration = std::time::Duration::from_secs(10);
/// How often a running game's exit, and a request to put it away, are looked for.
const POLL: std::time::Duration = std::time::Duration::from_millis(50);

/// Waits for `child`, putting it away when `put_away` says so.
fn wait(mut child: Child, tail: &Tail, put_away: impl Fn() -> bool) -> std::io::Result<Ended> {
    let ended = |status, put_away| Ended {
        status,
        put_away,
        stderr: tail.take(),
    };
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(ended(status, false));
        }
        if put_away() {
            eprintln!("dslot: asking the game to save and quit");
            terminate(&child);
            let deadline = std::time::Instant::now() + QUIT_GRACE;
            while std::time::Instant::now() < deadline {
                if let Some(status) = child.try_wait()? {
                    return Ok(ended(status, true));
                }
                std::thread::sleep(POLL);
            }
            eprintln!("dslot: the game did not quit; killed");
            child.kill()?;
            return Ok(ended(child.wait()?, true));
        }
        std::thread::sleep(POLL);
    }
}

/// Asks a child to quit: SIGTERM, on which an emulator saves its auto-resume state.
fn terminate(child: &Child) {
    // SAFETY: kill(2) on our own child's pid, which `child` keeps from being reused until it is
    // waited for.
    unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
}

/// Starts an emulator that goes back to a menu of its own when a game ends: its output is
/// watched for `cue`, the line that says so, and it is asked to quit then instead.
fn spawn_watched(mut command: Command, cue: String) -> std::io::Result<(Child, Arc<Tail>)> {
    let (log, out) = std::io::pipe()?;
    let child = command.stdout(out.try_clone()?).stderr(out).spawn()?;
    // Dropped here so that only the emulator (and what it starts) holds the pipe open.
    drop(command);
    let pid = child.id() as libc::pid_t;
    let tail = tee(log, Some((cue, pid)));
    Ok((child, tail))
}

/// Among the last lines a game wrote, the one that says why it stopped: the last that an
/// emulator marks as its reason (`launch:`, `gba:`, `zip:`, `rom:`, `could not`, and DSperate's
/// own crash line `dsperate: crashed`), else the last that says anything.
pub fn reason(lines: &[String]) -> Option<&str> {
    const MARKED: [&str; 6] = [
        "launch:",
        "gba:",
        "zip:",
        "rom:",
        "could not",
        "dsperate: crashed",
    ];
    let mut said = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty());
    said.clone()
        .rev()
        .find(|l| MARKED.iter().any(|m| l.starts_with(m)))
        .or_else(|| said.next_back())
}

/// Copies each seed file into place where there is none yet.
fn put_seeds(seed: &[Seed]) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    for s in seed {
        let to = match s.to.strip_prefix("~") {
            Ok(rest) => home.join(rest),
            Err(_) => s.to.clone(),
        };
        if to.exists() {
            continue;
        }
        let copied = to
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::copy(&s.from, &to));
        if let Err(e) = copied {
            eprintln!(
                "dslot: cannot copy {} to {}: {e}",
                s.from.display(),
                to.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rom::Console;
    use crate::settings::Legacy;
    use crate::settings::{console_scope, Scope};
    use crate::testing::TempDir;
    use toml::Value;

    fn game(console: Console, path: &str) -> Game {
        let path = PathBuf::from(path);
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        Game {
            console,
            path,
            title: vec![stem.clone()],
            stem,
            code: String::new(),
            icon: None,
            cover: None,
            gone: false,
        }
    }

    const DSPERATE: &str = r#"
        name = "DSperate"
        priority = 100
        [systems]
        NDS = ["/bin/sh", "{options}", "--saves", "{saves}", "--states", "{saves}", "{rom}"]
        GBA = ["/bin/sh", "--gba", "{options}", "--saves", "{saves}", "{rom}"]
        [env]
        DS_DEBUG = "marks"
        [[options]]
        key = "hires3d"
        label = "3D resolution"
        systems = ["NDS"]
        values = [1, 2, 3, 4]
        args = ["--hires3d", "{value}"]
        default = 2
        [[options]]
        key = "aa"
        label = "3D anti-aliasing"
        kind = "toggle"
        args = ["--aa"]
        off = ["--no-aa"]
        [[actions]]
        label = "Clear resume state"
        delete = ["{saves}/*.auto.dss"]
    "#;
    #[test]
    fn sega_and_lynx_nibcase_contracts() {
        for (id, system, ext) in [
            ("blastem", "MD", "md"),
            ("gearsystem", "GG", "gg"),
            ("handy", "LYNX", "lnx"),
        ] {
            let text = format!("name = \"Fixture\"\n[systems]\n{system} = [\"/usr/bin/nibcase\", \"--core\", \"/usr/lib/libretro/{id}_libretro.so\", \"--system\", \"{system}\", \"--saves\", \"{{saves}}\", \"{{rom}}\"]\n");
            let emulators = Emulators::assume_installed([(format!("nibcase-{id}"), text)]);
            let dir = TempDir::new("launch-cartridge");
            let settings = Settings::load(
                dir.path().join("settings.toml"),
                &Legacy::none(),
                &[],
                &emulators,
            );
            let path = format!("/r/{system}/Coast (World).{ext}.zip");
            let game = crate::rom::read_one(Path::new(&path)).unwrap();
            let emulator = emulator_for(&emulators, &settings, &game).unwrap();
            let launch = Launch::new(emulator, &game, &[], Some(Path::new("/r/Saves"))).unwrap();
            assert!(launch.args.windows(2).any(|w| w == ["--system", system]));
            assert_eq!(launch.args.last().unwrap(), &path);
            assert_eq!(
                launch.saves,
                Some(PathBuf::from(format!("/r/Saves/{system}/Coast (World)")))
            );
        }
    }

    #[test]
    fn nes_nibcase_contract() {
        let text = include_str!("../docs/examples/nibcase-nestopia.toml");
        let emulators = Emulators::assume_installed([("nibcase-nestopia".into(), text.into())]);
        let dir = TempDir::new("launch-nes");
        let settings = Settings::load(
            dir.path().join("settings.toml"),
            &Legacy::none(),
            &[],
            &emulators,
        );
        let game = crate::rom::read_one(Path::new("/r/FC/Star Lantern (World).nes.zip")).unwrap();
        let emulator = emulator_for(&emulators, &settings, &game).unwrap();
        let launch = Launch::new(emulator, &game, &[], Some(Path::new("/r/Saves"))).unwrap();
        assert_eq!(launch.program, "/usr/bin/nibcase");
        assert!(launch.args.windows(2).any(|w| w == ["--system", "FC"]));
        assert_eq!(
            launch.saves.as_deref(),
            Some(Path::new("/r/Saves/FC/Star Lantern (World)"))
        );
        assert_eq!(
            launch.args.last().unwrap(),
            "/r/FC/Star Lantern (World).nes.zip"
        );
    }

    #[test]
    fn snes_nibcase_contract() {
        let text = include_str!("../docs/examples/nibcase-snes9x.toml");
        nibkit::dslot::check("nibcase-snes9x", text).unwrap();
        let emulators = Emulators::assume_installed(vec![("nibcase-snes9x".into(), text.into())]);
        let dir = TempDir::new("launch-snes");
        let mut settings = Settings::load(
            dir.path().join("settings.toml"),
            &Legacy::none(),
            &[],
            &emulators,
        );
        // Keep the archive's inner extension out of settings keys and save paths.
        let game = crate::rom::read_one(Path::new("/r/SFC/Moon Bay (USA).smc.zip")).unwrap();
        assert_eq!(game.console, Console::Snes);
        assert_eq!(game.key(), "SFC/Moon Bay (USA)");
        settings.set(
            console_scope(Console::Snes),
            "nibcase-snes9x.args",
            Value::from("--option snes9x_region=ntsc"),
        );
        let emulator = emulator_for(&emulators, &settings, &game).unwrap();
        let words = options(&settings, emulator, &game);
        let launch = Launch::new(emulator, &game, &words, Some(Path::new("/r/Saves"))).unwrap();
        assert_eq!(launch.program, "/usr/bin/nibcase");
        assert_eq!(
            launch.args,
            [
                "--core",
                "/usr/lib/libretro/snes9x_libretro.so",
                "--system",
                "SFC",
                "--option",
                "snes9x_region=ntsc",
                "--saves",
                "/r/Saves/SFC/Moon Bay (USA)",
                "/r/SFC/Moon Bay (USA).smc.zip",
            ]
        );
        assert_eq!(
            launch.saves.as_deref(),
            Some(Path::new("/r/Saves/SFC/Moon Bay (USA)"))
        );
    }

    const MGBA: &str = r#"
        name = "mGBA"
        [systems]
        GBA = ["/bin/sh", "-L", "mgba_libretro.so", "--name={name}", "{rom}"]
    "#;

    struct Fixture {
        dir: TempDir,
        emulators: Emulators,
        settings: Settings,
    }

    fn fixture(name: &str) -> Fixture {
        let dir = TempDir::new(name);
        std::fs::create_dir_all(dir.path().join("emulators")).unwrap();
        std::fs::write(dir.path().join("emulators/dsperate.toml"), DSPERATE).unwrap();
        std::fs::write(dir.path().join("emulators/mgba.toml"), MGBA).unwrap();
        let emulators = Emulators::load(&[dir.path().join("emulators")]);
        let settings = Settings::load(
            dir.path().join("settings.toml"),
            &Legacy::none(),
            &[],
            &emulators,
        );
        Fixture {
            dir,
            emulators,
            settings,
        }
    }

    #[test]
    fn choosing() {
        let Fixture {
            emulators,
            mut settings,
            ..
        } = fixture("launch-choosing");
        let id = |s: &Settings, g: &Game| emulator_for(&emulators, s, g).map(|e| e.id.clone());
        let aw = game(Console::Gba, "/r/GBA/Advance Wars (USA).gba");
        let ds = game(Console::Ds, "/r/NDS/Mario Kart DS (USA).nds");
        assert_eq!(id(&settings, &aw).as_deref(), Some("dsperate"));
        settings.set(
            console_scope(Console::Gba),
            def::EMULATOR,
            Value::from("mgba"),
        );
        assert_eq!(id(&settings, &aw).as_deref(), Some("mgba"));
        settings.set(
            Scope::Game(aw.key()),
            def::EMULATOR,
            Value::from("dsperate"),
        );
        assert_eq!(id(&settings, &aw).as_deref(), Some("dsperate"));
        // A choice that doesn't play the console, or isn't installed, falls through.
        settings.set(Scope::Game(ds.key()), def::EMULATOR, Value::from("mgba"));
        settings.set(
            console_scope(Console::Ds),
            def::EMULATOR,
            Value::from("gone"),
        );
        assert_eq!(id(&settings, &ds).as_deref(), Some("dsperate"));
    }

    #[test]
    fn command() {
        let Fixture {
            emulators,
            mut settings,
            ..
        } = fixture("launch-command");
        let gs = game(Console::Ds, "/r/NDS/Golden Sun - Dark Dawn (USA).nds");
        let dsperate = emulators.get("dsperate").unwrap();
        // Nothing set: no options, the emulator decides.
        assert!(options(&settings, dsperate, &gs).is_empty());
        // Nor for the system: an emulator's options are a console's or a game's.
        settings.set(Scope::System, "dsperate.hires3d", Value::Integer(4));
        assert!(options(&settings, dsperate, &gs).is_empty());
        settings.set(
            console_scope(Console::Ds),
            "dsperate.hires3d",
            Value::Integer(2),
        );
        settings.set(Scope::Game(gs.key()), "dsperate.hires3d", Value::Integer(1));
        settings.set(
            console_scope(Console::Ds),
            "dsperate.aa",
            Value::Boolean(false),
        );
        settings.set(Scope::System, "dsperate.args", Value::from("--speed 100"));
        settings.set(
            Scope::Game(gs.key()),
            "dsperate.args",
            Value::from("--speed 150"),
        );
        let words = options(&settings, dsperate, &gs);
        assert_eq!(
            words,
            [
                "--hires3d",
                "1",
                "--no-aa",
                "--speed",
                "100",
                "--speed",
                "150"
            ]
        );

        let launch = Launch::new(
            dsperate,
            &gs,
            &words[..2],
            Some(Path::new("/mnt/roms/Saves")),
        )
        .unwrap();
        let saves = "/mnt/roms/Saves/NDS/Golden Sun - Dark Dawn (USA)";
        assert_eq!(launch.program, "/bin/sh");
        assert_eq!(
            launch.args,
            [
                "--hires3d",
                "1",
                "--saves",
                saves,
                "--states",
                saves,
                "/r/NDS/Golden Sun - Dark Dawn (USA).nds"
            ]
        );
        assert_eq!(launch.saves.as_deref(), Some(Path::new(saves)));
        assert_eq!(launch.env, [("DS_DEBUG".to_owned(), "marks".to_owned())]);

        // No options, and placeholders inside words.
        let aw = game(Console::Gba, "/r/GBA/Advance Wars (USA).zip");
        let launch = Launch::new(emulators.get("mgba").unwrap(), &aw, &[], None).unwrap();
        assert_eq!(
            launch.args,
            [
                "-L",
                "mgba_libretro.so",
                "--name=Advance Wars (USA)",
                "/r/GBA/Advance Wars (USA).zip"
            ]
        );
        assert_eq!(launch.saves, None);

        // {saves} where the platform has no saves folder; a console the emulator doesn't play.
        assert!(Launch::new(dsperate, &gs, &[], None).is_err());
        assert!(Launch::new(emulators.get("mgba").unwrap(), &gs, &[], None).is_err());
    }

    #[test]
    fn actions() {
        let Fixture { dir, emulators, .. } = fixture("launch-actions");
        let root = dir.path().join("Saves");
        let gs = game(Console::Ds, "/r/NDS/Golden Sun.nds");
        let saves = saves_dir(&root, &gs);
        std::fs::create_dir_all(&saves).unwrap();
        for f in ["Golden Sun.sav", "CEYE.auto.dss", "CEYE.1.dss"] {
            std::fs::write(saves.join(f), b"").unwrap();
        }
        let clear = &emulators.get("dsperate").unwrap().actions[0];
        assert!(can_do(clear, Some(&root)) && !can_do(clear, None));
        assert_eq!(do_action(clear, &gs, Some(&root)), 1);
        assert!(saves.join("Golden Sun.sav").exists() && !saves.join("CEYE.auto.dss").exists());

        delete_saves(&root, &gs).unwrap();
        assert!(!saves.exists() && root.join("NDS").exists());
        // Never anything but a game's own folder.
        let nameless = Game {
            stem: String::new(),
            ..game(Console::Ds, "/r/NDS/x.nds")
        };
        assert!(delete_saves(&root, &nameless).is_err());
        assert!(delete_saves(&root, &game(Console::Ds, "/r/NDS/...nds")).is_err());
        assert!(root.join("NDS").exists());
    }

    #[test]
    fn seeds() {
        let dir = TempDir::new("seeds");
        let from = dir.path().join("example.ini");
        std::fs::write(&from, "x").unwrap();
        let seed = Seed {
            from,
            to: dir.path().join("emu/settings.ini"),
        };
        put_seeds(std::slice::from_ref(&seed));
        assert!(seed.to.is_file());
        std::fs::write(&seed.to, "mine").unwrap();
        put_seeds(std::slice::from_ref(&seed));
        assert_eq!(std::fs::read_to_string(&seed.to).unwrap(), "mine");
    }

    #[test]
    fn apps() {
        let dir = TempDir::new("launch-app");
        let folder = dir.path().join("APPS/Tide Clock");
        std::fs::create_dir_all(&folder).unwrap();
        let text =
            "name = \"Tide Clock\"\ncommand = [\"./tide\", \"--big\"]\n[env]\nTIDE = \"1\"\n";
        let app = nibkit::dslot::App::parse(text).unwrap();
        let found = nibkit::dslot::Found {
            folder: folder.clone(),
            id: "Tide Clock".into(),
            app,
            icon: None,
        };
        let launch = Launch::app(&found).unwrap();
        assert_eq!(launch.program, folder.join("./tide").to_string_lossy());
        assert_eq!(launch.args, ["--big"]);
        assert_eq!(launch.dir.as_deref(), Some(folder.as_path()));
        assert_eq!(launch.env, [("TIDE".to_owned(), "1".to_owned())]);
        assert!(launch.saves.is_none());
    }

    #[test]
    fn watched_quits_on_cue() {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "echo starting; echo returning to menu; sleep 10; echo never",
        ]);
        let began = std::time::Instant::now();
        let (child, tail) = spawn_watched(command, "returning to menu".into()).unwrap();
        let ended = wait(child, &tail, || false).unwrap();
        assert!(!ended.status.success() && !ended.put_away);
        assert!(began.elapsed().as_secs() < 5);
        assert_eq!(ended.stderr, ["starting", "returning to menu"]);
    }

    #[test]
    fn the_last_lines_are_kept() {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "for i in $(seq 1 30); do echo line $i >&2; done; exit 3",
        ]);
        let (child, tail) = spawn_tailed(command).unwrap();
        let ended = wait(child, &tail, || false).unwrap();
        assert_eq!(ended.status.code(), Some(3));
        assert_eq!(ended.stderr.len(), TAIL);
        assert_eq!(ended.stderr.first().map(String::as_str), Some("line 11"));
        assert_eq!(ended.stderr.last().map(String::as_str), Some("line 30"));
    }

    #[test]
    fn the_reason_is_the_marked_line_else_the_last() {
        let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(reason(&[]), None);
        assert_eq!(reason(&lines(&["", "  "])), None);
        // A marked line wins over later chatter.
        assert_eq!(
            reason(&lines(&[
                "dsperate: starting",
                "launch: Game.nds cannot start: ZIP NOT SUPPORTED",
                "shutting down audio",
                ""
            ])),
            Some("launch: Game.nds cannot start: ZIP NOT SUPPORTED")
        );
        // The last marked line, of several.
        assert_eq!(
            reason(&lines(&["rom: header read", "gba: bad header", "bye"])),
            Some("gba: bad header")
        );
        assert_eq!(
            reason(&lines(&["  could not open the ROM  ", "exiting"])),
            Some("could not open the ROM")
        );
        // Nothing marked: the last line that says anything.
        assert_eq!(
            reason(&lines(&["Segmentation fault", "", "   "])),
            Some("Segmentation fault")
        );
        // DSperate's crash line, written from its signal handler, over the
        // launch marks before it and whatever the runtime said after.
        assert_eq!(
            reason(&lines(&[
                "dsperate: 12.345 s displays open",
                "dsperate: crashed (SIGBUS) after \"displays open\", loading /mnt/ds/Roms/NDS/X.nds",
                "Bus error"
            ])),
            Some("dsperate: crashed (SIGBUS) after \"displays open\", loading /mnt/ds/Roms/NDS/X.nds")
        );
    }

    #[test]
    fn put_away_on_request() {
        // A game that saves on SIGTERM and exits 0, asked to go at once.
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "trap 'echo saved; exit 0' TERM; sleep 10 & wait"]);
        // Asked once it has started (and set its trap), as a long press would come mid-game.
        let started = std::time::Instant::now();
        let asked = || started.elapsed() > std::time::Duration::from_millis(300);
        let (child, tail) = spawn_tailed(command).unwrap();
        let ended = wait(child, &tail, asked).unwrap();
        assert!(ended.put_away && ended.status.success(), "{ended:?}");
        // Nobody asking: it runs to its end.
        let (child, tail) = spawn_tailed(Command::new("true")).unwrap();
        let quick = wait(child, &tail, || false).unwrap();
        assert!(!quick.put_away && quick.status.success());
        assert!(quick.stderr.is_empty());
    }
}
