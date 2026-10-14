mod art;
mod boot;
mod gfx;
mod loader;
mod rom;
mod settings;
mod stats;
mod touch;
mod ui;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Instant;

const USAGE: &str = "usage: dslot [--roms DIR]... [--launcher FILE] [--swap]
             [--shot FILE.png] [--press KEYS] [--sounds DIR] [--make-demo DIR]

  --roms DIR       folder of .nds games (repeatable; default: both cards' Roms/NDS)
  --launcher FILE  program started with the ROM path (default: the stock DS emulator)
  --swap           put the cart screen on the other panel
  --shot FILE      draw one frame to a PNG and exit
  --press KEYS     with --shot: keys to press first, e.g. Tab,Down,Right
  --sounds DIR     compare sound sets in DIR/1, DIR/2, ... with the number keys
  --make-demo DIR  write a few tiny fake ROMs for trying the UI";

const DEFAULT_ROMS: [&str; 2] = ["/mnt/sdcard/Roms/NDS", "/mnt/mmc/Roms/NDS"];
const DEFAULT_LAUNCHER: &str = "/mnt/vendor/deep/nnddss/launch.sh";
// What the stock emulator logs once a game has shut down and it is about to show its own menu.
const BACK_TO_MENU: &str = "returning to NNDDSS menu";

/// Logs a stage of start-up against the kernel's clock, to show where the time goes.
pub fn mark(stage: &str) {
    let up = std::fs::read_to_string("/proc/uptime").unwrap_or_default();
    eprintln!("dslot: {} s {stage}", up.split_whitespace().next().unwrap_or("?"));
}

fn main() {
    if let Err(e) = run() {
        eprintln!("dslot: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut roms: Vec<PathBuf> = Vec::new();
    let mut launcher = PathBuf::from(DEFAULT_LAUNCHER);
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let mut opts = ui::Opts {
        cache: exe_dir.join("cache"),
        boot_script: exe_dir.join("boot.sh"),
        start: 0,
        returning: false,
        sounds: None,
        swap: std::env::var("DSLOT_SWAP").is_ok_and(|v| v == "1"),
        shot: None,
        press: Vec::new(),
        searched: Vec::new(),
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().map(PathBuf::from).ok_or(format!("{arg} needs a value\n{USAGE}"));
        match arg.as_str() {
            "--roms" => roms.push(value()?),
            "--launcher" => launcher = value()?,
            "--shot" => opts.shot = Some(value()?),
            "--press" => opts.press = value()?.to_string_lossy().split(',').map(str::to_owned).collect(),
            "--sounds" => opts.sounds = Some(value()?),
            "--swap" => opts.swap = true,
            "--make-demo" => return rom::make_demo(&value()?).map_err(|e| e.to_string()),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("unknown option {arg}\n{USAGE}")),
        }
    }
    if roms.is_empty() {
        roms = DEFAULT_ROMS.iter().map(PathBuf::from).collect();
    }

    mark("started");
    boot::refresh(&opts.boot_script);
    eprintln!("dslot: start at power-on: available={} enabled={}", boot::available(), boot::enabled(&opts.boot_script));
    let fonts = gfx::Fonts::load();
    mark("fonts loaded");
    let shell = art::Shell::load();
    mark("cart shell loaded");
    // Started ahead of the stock boot script (see early.sh), which is what mounts the games
    // card: it may be a moment behind.
    if let Ok(dir) = std::env::var("DSLOT_WAIT_FOR") {
        wait_for_mount(&dir);
        mark("games card mounted");
    }
    let games = rom::scan(&roms, &opts.cache.join("games.bin"));
    mark("games scanned");
    eprintln!("dslot: {} games in {:?}", games.len(), roms);
    opts.searched = roms;

    let mut stats = stats::Stats::load(exe_dir.join("stats.tsv"));
    let mut settings = settings::Settings::load(exe_dir.join("settings.ini"));
    mark("stats and settings loaded");

    loop {
        // The window is dropped before the game starts and rebuilt when it exits.
        match ui::run(&games, &mut stats, &mut settings, &fonts, &shell, &opts)? {
            ui::Outcome::Quit => return Ok(()),
            ui::Outcome::PowerOff => {
                // As the stock menu does it. Only on the device: a desktop preview just closes.
                if std::path::Path::new("/mnt/vendor").is_dir() {
                    let _ = Command::new("sync").status();
                    let _ = Command::new("poweroff").status();
                    // Stay up while the system goes down, so the stock menu does not flash past.
                    std::thread::sleep(std::time::Duration::from_secs(20));
                }
                return Ok(());
            }
            ui::Outcome::Launch(i) => {
                let rom = &games[i].path;
                let began = Instant::now();
                match play(&launcher, rom) {
                    Ok(status) => eprintln!("dslot: {} exited with {status}", rom.display()),
                    Err(e) => eprintln!("dslot: cannot start {}: {e}", launcher.display()),
                }
                stats.record(rom, began.elapsed().as_secs());
                opts.start = i;
                opts.returning = true;
            }
        }
    }
}

/// Returns once something is mounted at `dir`, or after three seconds.
fn wait_for_mount(dir: &str) {
    for _ in 0..150 {
        let mounted = std::fs::read_to_string("/proc/mounts").is_ok_and(|m| m.lines().any(|l| l.split(' ').nth(1) == Some(dir)));
        if mounted {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Runs the game and waits for it. The stock emulator falls back to its own menu when a game
/// ends, so its log is watched for that moment and it is told to quit instead.
fn play(launcher: &Path, rom: &Path) -> std::io::Result<ExitStatus> {
    let (log, out) = std::io::pipe()?;
    let mut child = Command::new(launcher).arg(rom).stdout(out.try_clone()?).stderr(out).spawn()?;
    let pid = child.id() as libc::pid_t;
    // Not joined: helpers the launcher leaves running keep the pipe open after the game is gone.
    std::thread::spawn(move || {
        for line in BufReader::new(log).split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&line);
            eprintln!("{line}");
            if line.contains(BACK_TO_MENU) {
                // SIGTERM, not SIGKILL: the emulator restores the GPU governor on the way out.
                unsafe { libc::kill(pid, libc::SIGTERM) };
            }
        }
    });
    child.wait()
}
