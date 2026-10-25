//! DSlot: a game shelf for the Anbernic RG DS+. Boxes and details on the top screen, carts
//! over a slot on the touch screen; pushing a cart in starts the game in its emulator, and
//! leaving the game brings the shelf back. docs/architecture.md has the map of the code.

mod art;
mod device;
mod emulator;
mod folders;
mod gfx;
mod kms;
mod launch;
mod library;
mod loader;
mod platform;
mod rescan;
mod resume;
mod rom;
mod settings;
mod splash;
mod stats;
mod systems;
#[cfg(test)]
mod testing;
mod update;

mod ui;

use emulator::Emulators;
use launch::{Ended, Launch};
use nibkit::power::Requests;
use rom::{Console, Game};
use settings::Settings;
use std::path::{Path, PathBuf};
use std::time::Instant;

const USAGE: &str = "usage: dslot [--roms DIR]... [--swap] [--shot FILE.png] [--press KEYS]
             [--make-demo DIR]

  --roms DIR       a Roms folder, with a folder for each console in it (NDS, GBA, ...);
                   repeatable (default: the cards' Roms folders)
  --swap           put the cart screen on the other panel
  --shot FILE      draw one frame to a PNG and exit
  --press KEYS     with --shot: keys to press first, e.g. Tab,Down,Right
  --make-demo DIR  write a Roms folder of tiny fake games for trying the shelf
  --launch ROM     start one game as the shelf would (its emulator, settings and saves
                   folder), without the shelf; exits with the emulator's status

  DSLOT_KMS=0      no skeleton at start-up (on nibOS: the shelf's layout straight through
                   KMS while SDL and GL start)
  DSLOT_PRELOAD=0  leave loading Mesa to SDL rather than starting it at once
  DSLOT_WAIT_FOR   a mount point to wait for (up to 3 s) before listing games
  DSLOT_SWAP=1     as --swap
  DSLOT_FPS=1      log the frame rate";

/// Logs a stage of start-up against the kernel's clock, to show where the time goes.
/// The clock is CLOCK_BOOTTIME, to the millisecond (/proc/uptime only gives hundredths).
pub fn mark(stage: &str) {
    match uptime() {
        Some(t) => eprintln!("dslot: {t:.3} s {stage}"),
        None => eprintln!("dslot: ? s {stage}"),
    }
}

#[cfg(target_os = "linux")]
fn uptime() -> Option<f64> {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes the timespec it is given and nothing else.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut t) } == 0;
    ok.then(|| t.tv_sec as f64 + t.tv_nsec as f64 / 1e9)
}

#[cfg(not(target_os = "linux"))]
fn uptime() -> Option<f64> {
    None
}

fn main() {
    if let Err(e) = run() {
        eprintln!("dslot: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let dirs = platform::dirs();
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut opts = ui::Opts {
        cache: dirs.cache.clone(),
        swap: std::env::var("DSLOT_SWAP").is_ok_and(|v| v == "1"),
        shot: None,
        press: Vec::new(),
        searched: Default::default(),
        outputs: std::sync::Mutex::new(None),
        last_frame: std::sync::Mutex::new(None),
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .map(PathBuf::from)
                .ok_or(format!("{arg} needs a value\n{USAGE}"))
        };
        match arg.as_str() {
            "--roms" => roots.push(value()?),
            "--shot" => opts.shot = Some(value()?),
            "--press" => {
                opts.press = value()?
                    .to_string_lossy()
                    .split(',')
                    .map(str::to_owned)
                    .collect()
            }
            "--swap" => opts.swap = !opts.swap,
            "--make-demo" => return rom::make_demo(&value()?).map_err(|e| e.to_string()),
            "--launch" => return launch_one(&value()?, &dirs),
            // For the docs: the start-up skeleton, both screens stacked, to a PNG.
            "--skeleton-shot" => {
                let (top, bot) = ui::skeleton(&gfx::Fonts::load());
                let mut px = top.px.clone();
                px.extend_from_slice(&bot.px);
                return gfx::save_png(&value()?, top.w as u32, (top.h + bot.h) as u32, &px);
            }
            // For packaging: nibOS's power-on picture (splash.rs).
            "--write-splash" => return splash::write(&value()?, 1024, 768),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("unknown option {arg}\n{USAGE}")),
        }
    }
    if roots.is_empty() {
        roots = platform::rom_roots();
    }
    // A console has a page when an emulator for it is installed
    // (anywhere but the device, for trying the shelf out, the first is enough). Its games are
    // looked for in its folder in every Roms folder, as the stock OS lays the cards out:
    // Roms/NDS, Roms/GBA, ...
    let mut emulators = Emulators::load(&dirs.emulators);
    // The game folders a card should have, and doesn't yet (folders.rs).
    folders::make_soon(&roots, &emulators);
    let playable =
        |c: Console| !platform::on_device() || emulators.for_system(c.folder()).next().is_some();
    for console in Console::ALL.into_iter().filter(|&c| playable(c)) {
        opts.searched[console as usize] = roots.iter().map(|r| r.join(console.folder())).collect();
    }

    mark("started");
    platform::check_user()?;
    // A long press of power (nibOS) asks for the power menu, from now on.
    let power = nibkit::power::watch();
    platform::refresh_power_on_hook();
    let on_device = platform::on_device();
    let hold_outputs = platform::SKELETON_START && on_device && opts.shot.is_none();
    let skeleton = hold_outputs && !std::env::var("DSLOT_KMS").is_ok_and(|v| v == "0");
    // Mesa is loaded from the start, on a thread, rather than when SDL makes the renderers: on
    // nibOS the shelf is up ~0.15 s sooner (DSLOT_PRELOAD=0 for the other way). The skeleton
    // needs no dlopen, so it isn't held up.
    let preload = on_device && !std::env::var("DSLOT_PRELOAD").is_ok_and(|v| v == "0");
    let gl = preload.then(|| {
        std::thread::spawn(|| {
            platform::preload_gl();
            mark("GL libraries loaded");
        })
    });
    let fonts = gfx::Fonts::load();
    mark("fonts loaded");
    let splash = on_device && platform::splash_shown();
    if splash {
        mark("splash on screen");
    }
    // DSlot's own hold on the screens (kms), opened now, while no one else has them, and kept:
    // the shelf's layout, empty, at once (the shelf comes up over it), and later the shelf's
    // last frame between it and a game.
    if hold_outputs {
        match kms::Kms::open(opts.swap) {
            Ok(mut out) => {
                if skeleton && !splash {
                    let (top, bot) = ui::skeleton(&fonts);
                    out.show(&[&top, &bot])?;
                    mark("skeleton shown");
                } else if let (true, Some([top, bot])) =
                    (splash, splash::read(platform::SPLASH_PICTURES))
                {
                    // The splash's picture again, in DSlot's buffers: nothing changes on screen,
                    // but what SDL finds there when it starts, and puts back when it lets go of
                    // the screens for a game, is now DSlot's, which lasts. The splash program's
                    // goes when it exits (as DSlot's first frame is up), and SDL putting that
                    // back failed ("Could not restore CRTC"), leaving a screen dark for a moment.
                    match out.show(&[&top, &bot]) {
                        Ok(()) => mark("splash taken over"),
                        Err(e) => eprintln!("dslot: KMS: {e}"),
                    }
                }
                *opts.outputs.lock().unwrap_or_else(|e| e.into_inner()) = Some(out);
            }
            Err(e) => eprintln!("dslot: KMS: {e}"),
        }
    }

    // The games are listed (which may wait ~1 s at power-on for the games card to mount) and
    // the cart shell is drawn while SDL, the windows and GL start here: SDL stays on this
    // thread. Each page's games are listed on a thread of their own.
    let listings: Vec<_> = Console::ALL
        .into_iter()
        .filter(|&s| !opts.searched[s as usize].is_empty())
        .map(|console| {
            let (dirs, known) = (
                opts.searched[console as usize].clone(),
                opts.cache.join(console.known()),
            );
            std::thread::spawn(move || {
                // A mount point the games are on that may still be on its way (nibOS mounts
                // the ROMS partition early, beside DSlot rather than before it).
                if let Ok(dir) = std::env::var("DSLOT_WAIT_FOR") {
                    wait_for_mount(&dir);
                }
                let games = rom::scan(console, &dirs, &known);
                mark(&format!("{} games scanned", console.label()));
                games
            })
        })
        .collect();
    let shell = std::thread::spawn(|| {
        let shell = art::Shell::load();
        mark("cart shell loaded");
        shell
    });
    // SDL's start needs the loader's lock, which dlopen holds.
    if let Some(t) = gl {
        let _ = t.join();
    }
    let mut screens = Some(ui::open(&opts)?);
    let mut games: Vec<Game> = Vec::new();
    for listing in listings {
        let found = listing.join().map_err(|_| "listing the games failed")?;
        if let Some(g) = found.first() {
            eprintln!(
                "dslot: {} {} games in {:?}",
                found.len(),
                g.console.label(),
                opts.searched[g.console as usize]
            );
        }
        games.extend(found);
    }
    rom::name_from_lists(&mut games, &emulators);
    // The apps in Roms/APPS on either card: their own page, when there are any.
    let mut apps = nibkit::dslot::apps(&roots);
    if !apps.is_empty() {
        eprintln!("dslot: {} apps", apps.len());
    }
    games.extend(rom::apps(&apps));
    let shell = shell.join().map_err(|_| "drawing the cart shell failed")?;

    // Games copied onto the cards or deleted from them from now on, and cards put in or out.
    let folders = |opts: &ui::Opts| rescan::Folders {
        searched: opts.searched.clone(),
        cache: opts.cache.clone(),
    };
    let rescan = rescan::Rescan::start(folders(&opts));

    let mut stats = stats::Stats::load(dirs.stats.clone());
    let mut settings = Settings::load(dirs.settings.clone(), &dirs.legacy, &games, &emulators);
    mark("stats and settings loaded");

    let mut back = None;
    let mut refresh = false;
    loop {
        if refresh {
            let put_away = back.is_some_and(|b: ui::Back| b.put_away);
            let selected = back
                .and_then(|b: ui::Back| games.get(b.game))
                .map(|g| g.path.clone());
            emulators = Emulators::load(&dirs.emulators);
            folders::make_soon(&roots, &emulators);
            games.clear();
            for console in Console::ALL {
                let playable = !platform::on_device()
                    || emulators.for_system(console.folder()).next().is_some();
                opts.searched[console as usize] = if playable {
                    roots.iter().map(|r| r.join(console.folder())).collect()
                } else {
                    Vec::new()
                };
                if !opts.searched[console as usize].is_empty() {
                    games.extend(rom::scan(
                        console,
                        &opts.searched[console as usize],
                        &opts.cache.join(console.known()),
                    ));
                }
            }
            rom::name_from_lists(&mut games, &emulators);
            apps = nibkit::dslot::apps(&roots);
            games.extend(rom::apps(&apps));
            settings.reload();
            back = selected
                .and_then(|path| games.iter().position(|g| g.path == path))
                .map(|game| ui::Back { game, put_away });
            rescan.watch(folders(&opts));
            mark("emulator integrations refreshed");
        }
        let shelf = ui::Shelf {
            emulators: &emulators,
            emulator_dirs: &dirs.emulators,
            fonts: &fonts,
            shell: &shell,
            opts: &opts,
            power: &power,
            rescan: &rescan,
        };
        // The windows are dropped before the game starts and made again when it exits.
        let open = match screens.take() {
            Some(s) => s,
            None => ui::open(&opts)?,
        };
        match ui::run(open, &shelf, &mut games, back, &mut stats, &mut settings)? {
            ui::Outcome::Refresh(selected) => {
                hold_last_frame(&opts);
                back = selected.map(|game| ui::Back {
                    game,
                    put_away: false,
                });
                refresh = true;
            }
            ui::Outcome::Quit => return Ok(()),
            ui::Outcome::PowerOff => {
                platform::power_off();
                return Ok(());
            }
            ui::Outcome::Restart => {
                platform::restart();
                return Ok(());
            }
            ui::Outcome::Launch(i) | ui::Outcome::Resume(i) => {
                let game = &games[i];
                let began = Instant::now();
                // The shelf's windows are gone: its last frame stays up until the game's first.
                hold_last_frame(&opts);
                mark("game starting");
                let played = if game.is_app() {
                    start_app(game, &apps, &power)
                } else {
                    play(game, &emulators, &settings, &power)
                };
                let put_away = match played {
                    Ok(ended) => ended.put_away,
                    Err(e) => {
                        eprintln!("dslot: {e}");
                        false
                    }
                };
                mark("game exited");
                // And until the shelf's first frame again.
                hold_last_frame(&opts);
                // An emulator may set its own options in DSlot's settings, from its own menu.
                settings.reload();
                stats.record(&game.path, began.elapsed().as_secs());
                back = Some(ui::Back { game: i, put_away });
                refresh = emulator::signature(&emulators)
                    != emulator::signature(&Emulators::load(&dirs.emulators));
                // PortMaster has been getting ports (Roms/PORTS may be new), or an app has
                // installed itself and goes by its own name now: look at everything again.
                let installs = apps
                    .iter()
                    .any(|f| rom::app_path(f) == game.path && f.app.until.is_some());
                if game.is_app() && (game.console == Console::Ports || installs) {
                    refresh = true;
                }
            }
        }
    }
}

/// Puts the shelf's last frame up through DSlot's own hold on the screens, for the moment no
/// one else has them, then lets them go to whoever takes them next.
fn hold_last_frame(opts: &ui::Opts) {
    let outputs = opts.outputs.lock().unwrap_or_else(|e| e.into_inner());
    let painted = opts
        .last_frame
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some();
    if let (Some(out), true) = (outputs.as_ref(), painted) {
        if out.take() {
            if let Err(e) = out.set() {
                eprintln!("dslot: KMS: {e}");
            }
            out.release();
        }
    }
}

/// Starts `game` in its emulator and waits for it to exit, or puts it away when the power
/// menu is asked for.
fn play(
    game: &Game,
    emulators: &Emulators,
    settings: &Settings,
    power: &Requests,
) -> Result<Ended, String> {
    let emulator = launch::emulator_for(emulators, settings, game).ok_or_else(|| {
        format!(
            "no emulator for {} games here (docs/emulators.md)",
            game.console.label()
        )
    })?;
    let options = launch::options(settings, emulator, game);
    let launch = Launch::new(emulator, game, &options, platform::saves().as_deref())?;
    let ended = launch
        .run(|| power.take())
        .map_err(|e| format!("cannot start {}: {e}", launch.program))?;
    eprintln!(
        "dslot: {} exited with {}",
        game.path.display(),
        ended.status
    );
    Ok(ended)
}

/// Starts an app from Roms/APPS and waits for it to exit, or puts it away for the power menu.
fn start_app(
    game: &Game,
    apps: &[nibkit::dslot::Found],
    power: &Requests,
) -> Result<Ended, String> {
    let found = apps
        .iter()
        .find(|f| rom::app_path(f) == game.path)
        .ok_or("the app has gone")?;
    let launch = Launch::app(found)?;
    let ended = launch
        .run(|| power.take())
        .map_err(|e| format!("cannot start {}: {e}", launch.program))?;
    eprintln!("dslot: {} exited with {}", found.app.name, ended.status);
    Ok(ended)
}

/// `dslot --launch ROM`: one game, as the shelf would start it, then DSlot's exit status is
/// the emulator's. Its play time is not counted.
fn launch_one(rom: &Path, dirs: &platform::Dirs) -> Result<(), String> {
    let game =
        rom::read_one(rom).ok_or_else(|| format!("{}: not a ROM DSlot knows", rom.display()))?;
    let emulators = Emulators::load(&dirs.emulators);
    let settings = Settings::load(
        dirs.settings.clone(),
        &dirs.legacy,
        std::slice::from_ref(&game),
        &emulators,
    );
    let ended = play(&game, &emulators, &settings, &nibkit::power::watch())?;
    std::process::exit(ended.status.code().unwrap_or(1));
}

/// Returns once something is mounted at `dir`, or after three seconds.
fn wait_for_mount(dir: &str) {
    for _ in 0..300 {
        let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
        if mounts.lines().any(|l| l.split(' ').nth(1) == Some(dir)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
