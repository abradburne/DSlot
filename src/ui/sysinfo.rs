//! The System page's **System info** page: a read-only page of the device and what it is
//! using, for a player asked "what does it say under System info?" and for anyone chasing
//! memory on a 1 GB device. The board and the nibOS release, the kernel, the uptime, the A/B
//! slot running, DSlot's and nibkit's versions; the memory (in use, available, DSlot's own,
//! the GPU's shared memory, the file cache, the swap on zram, the kernel's CMA pool); the CPU
//! (load, each core's frequency, the governor, the temperatures, the GPU's frequency); the
//! storage (the system, the NIBOS-DATA partition, the TF2 card while one is mounted); and the
//! panels' modes.
//!
//! Every fact is read from `/proc` and `/sys` as any user may, nothing is run, and a fact whose
//! source isn't there (a desktop preview, the stock OS, a card that isn't nibOS) is left out
//! rather than guessed: a row is only shown with a value. A [`Watch`] reads them on a thread
//! every second while the page is up.

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// How often the facts are read again while the page is up.
const EVERY: Duration = Duration::from_secs(1);
const RELEASE: &str = "/usr/lib/nibos/release";
const MODEL: &str = "/sys/firmware/devicetree/base/model";
const CPUS: &str = "/sys/devices/system/cpu";
const THERMAL: &str = "/sys/class/thermal";
const DEVFREQ: &str = "/sys/class/devfreq";
const DRM: &str = "/sys/class/drm";
/// The file systems shown, by mount point, and how each is named.
const VOLUMES: [(&str, &str); 3] = [
    ("/", "System"),
    ("/mnt/roms", "NIBOS-DATA"),
    ("/mnt/ds", "TF2 card"),
];

/// The nibOS release, out of `/usr/lib/nibos/release`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub build: String,
    pub board: String,
    pub edition: String,
    pub channel: String,
}

/// The memory, in kB, out of `/proc/meminfo`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
    /// Shmem: what the GPU driver and the like share through tmpfs.
    pub shared: u64,
    /// The file cache: Cached and Buffers, less what Cached counts of Shmem.
    pub cache: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    /// The kernel's CMA pool, where it has one.
    pub cma: Option<(u64, u64)>,
}

impl Memory {
    /// What is in use: the total, less what could be had back.
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

/// One thermal zone, in millidegrees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    pub name: String,
    pub millideg: i64,
}

/// A file system: bytes used and in all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub name: &'static str,
    pub used: u64,
    pub total: u64,
}

/// The device as last read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// Read at least once: before that every field is a blank, not a fact.
    pub read: bool,
    /// The board, as the device tree names it ("Anbernic RG DS+").
    pub board: Option<String>,
    pub release: Option<Release>,
    pub kernel: Option<String>,
    pub uptime: Option<u64>,
    /// The root partition's label on the kernel's command line ("nibos-root-b").
    pub root: Option<String>,
    pub memory: Option<Memory>,
    /// The swap device's name ("zram0"), where there is one.
    pub swap: Option<String>,
    /// DSlot's own resident size, in kB.
    pub rss: Option<u64>,
    /// The load averages over 1, 5 and 15 minutes.
    pub load: Option<[f32; 3]>,
    /// Each core's frequency, in kHz.
    pub cores: Vec<u64>,
    pub governor: Option<String>,
    pub zones: Vec<Zone>,
    /// The GPU's frequency in Hz and its governor, where devfreq has it.
    pub gpu: Option<(u64, String)>,
    pub volumes: Vec<Volume>,
    /// Each connected panel: its connector and its mode ("DSI-1", "1024x768").
    pub panels: Vec<(String, String)>,
}

impl Info {
    /// The title line: what the device is, in a few words.
    pub fn summary(&self) -> String {
        match (&self.release, &self.board) {
            (Some(r), Some(b)) => format!("nibOS {} on {b}", r.version),
            (Some(r), None) => format!("nibOS {}", r.version),
            (None, Some(b)) => b.clone(),
            (None, None) if self.read => "This device".into(),
            (None, None) => "Reading\u{2026}".into(),
        }
    }

    /// The A/B slot running, from the root partition's label: "A" or "B".
    pub fn slot(&self) -> Option<&'static str> {
        match self.root.as_deref()?.rsplit('-').next()? {
            "a" => Some("A"),
            "b" => Some("B"),
            _ => None,
        }
    }
}

struct Shared {
    info: Mutex<Info>,
    /// Set to stop the thread; the condvar wakes it.
    stop: Mutex<bool>,
    wake: Condvar,
}

/// The facts, read on a thread every second for as long as it is held.
pub struct Watch {
    shared: Arc<Shared>,
}

impl Watch {
    /// Starts reading; `None` only if the thread can't be started.
    pub fn start() -> Option<Watch> {
        let shared = Arc::new(Shared {
            info: Mutex::new(Info::default()),
            stop: Mutex::new(false),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("sysinfo".into())
            .spawn(move || loop {
                let found = read();
                *lock(&worker.info) = found;
                let stop = lock(&worker.stop);
                let (stop, _) = worker
                    .wake
                    .wait_timeout(stop, EVERY)
                    .unwrap_or_else(|e| e.into_inner());
                if *stop {
                    break;
                }
            })
            .ok()?;
        Some(Watch { shared })
    }

    /// The facts as last read.
    pub fn now(&self) -> Info {
        lock(&self.shared.info).clone()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        *lock(&self.shared.stop) = true;
        self.shared.wake.notify_all();
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn text(path: impl AsRef<Path>) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let s = s.trim_end_matches('\0').trim();
    (!s.is_empty()).then(|| s.to_owned())
}

fn number(path: impl AsRef<Path>) -> Option<u64> {
    text(path)?.parse().ok()
}

/// Everything, as the system has it now.
fn read() -> Info {
    let meminfo = text("/proc/meminfo").unwrap_or_default();
    let (cores, governor) = cpufreq(Path::new(CPUS));
    Info {
        read: true,
        board: text(MODEL),
        release: text(RELEASE).map(|s| release(&s)),
        kernel: text("/proc/sys/kernel/osrelease"),
        uptime: text("/proc/uptime").and_then(|s| uptime(&s)),
        root: text("/proc/cmdline").and_then(|s| root_label(&s)),
        memory: memory(&meminfo),
        swap: text("/proc/swaps").and_then(|s| swap(&s)),
        rss: text("/proc/self/status").and_then(|s| rss(&s)),
        load: text("/proc/loadavg").and_then(|s| load(&s)),
        cores,
        governor,
        zones: zones(Path::new(THERMAL)),
        gpu: gpu(Path::new(DEVFREQ)),
        volumes: volumes(),
        panels: panels(Path::new(DRM)),
    }
}

/// The release file's KEY=value lines.
pub fn release(s: &str) -> Release {
    let mut r = Release::default();
    for line in s.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        match key.trim() {
            "NIBOS_VERSION" => r.version = value,
            "NIBOS_BUILD" => r.build = value,
            "NIBOS_BOARD" => r.board = value,
            "NIBOS_EDITION" => r.edition = value,
            "NIBOS_CHANNEL" => r.channel = value,
            _ => {}
        }
    }
    r
}

/// `/proc/uptime`'s first number, in whole seconds.
pub fn uptime(s: &str) -> Option<u64> {
    let secs: f64 = s.split_whitespace().next()?.parse().ok()?;
    Some(secs as u64)
}

/// The root partition's label from the kernel's command line (`root=PARTLABEL=...`).
pub fn root_label(cmdline: &str) -> Option<String> {
    cmdline
        .split_whitespace()
        .find_map(|arg| arg.strip_prefix("root=PARTLABEL="))
        .map(str::to_owned)
}

/// A `Key:   123 kB` line's number out of `/proc/meminfo` or `/proc/self/status`.
fn kb(s: &str, key: &str) -> Option<u64> {
    s.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim() == key).then(|| v.split_whitespace().next()?.parse().ok())?
    })
}

/// The memory out of `/proc/meminfo`.
pub fn memory(s: &str) -> Option<Memory> {
    let total = kb(s, "MemTotal")?;
    let available = kb(s, "MemAvailable")?;
    let shared = kb(s, "Shmem").unwrap_or(0);
    let cached = kb(s, "Cached").unwrap_or(0) + kb(s, "Buffers").unwrap_or(0);
    let swap_total = kb(s, "SwapTotal").unwrap_or(0);
    let swap_free = kb(s, "SwapFree").unwrap_or(swap_total);
    let cma = match (kb(s, "CmaTotal"), kb(s, "CmaFree")) {
        (Some(t), Some(f)) if t > 0 => Some((t.saturating_sub(f), t)),
        _ => None,
    };
    Some(Memory {
        total,
        available,
        shared,
        cache: cached.saturating_sub(shared),
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
        cma,
    })
}

/// The first swap device's name out of `/proc/swaps` ("zram0").
pub fn swap(s: &str) -> Option<String> {
    let dev = s.lines().nth(1)?.split_whitespace().next()?;
    Some(dev.rsplit('/').next().unwrap_or(dev).to_owned())
}

/// VmRSS out of `/proc/self/status`, in kB.
pub fn rss(status: &str) -> Option<u64> {
    kb(status, "VmRSS")
}

/// The three load averages out of `/proc/loadavg`.
pub fn load(s: &str) -> Option<[f32; 3]> {
    let mut parts = s.split_whitespace().map(str::parse::<f32>);
    Some([
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ])
}

/// Each core's current frequency in kHz, in order, and cpu0's governor.
fn cpufreq(cpus: &Path) -> (Vec<u64>, Option<String>) {
    let mut cores = Vec::new();
    for n in 0.. {
        let dir = cpus.join(format!("cpu{n}/cpufreq"));
        if !dir.is_dir() {
            break;
        }
        let Some(khz) = number(dir.join("scaling_cur_freq")) else {
            break;
        };
        cores.push(khz);
    }
    let governor = text(cpus.join("cpu0/cpufreq/scaling_governor"));
    (cores, governor)
}

/// The thermal zones, each by its type less "-thermal" ("cpu", "gpu").
fn zones(thermal: &Path) -> Vec<Zone> {
    let mut found: Vec<(String, Zone)> = std::fs::read_dir(thermal)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .filter_map(|e| {
            let dir = e.path();
            let kind = text(dir.join("type"))?;
            let millideg = text(dir.join("temp"))?.parse().ok()?;
            let name = kind.trim_end_matches("-thermal").to_owned();
            Some((
                e.file_name().to_string_lossy().into_owned(),
                Zone { name, millideg },
            ))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(_, z)| z).collect()
}

/// The GPU's devfreq: its frequency in Hz and governor.
fn gpu(devfreq: &Path) -> Option<(u64, String)> {
    std::fs::read_dir(devfreq)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().contains("gpu"))
        .and_then(|dir| {
            let hz = number(dir.join("cur_freq"))?;
            let governor = text(dir.join("governor")).unwrap_or_default();
            Some((hz, governor))
        })
}

/// Whether a file system is mounted at `dir`.
fn mounted(dir: &str) -> bool {
    std::fs::read_to_string("/proc/self/mounts")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split(' ').nth(1))
        .any(|m| m == dir)
}

/// The volumes shown: the root, and each mount point while something is mounted there (an
/// unmounted TF2 slot is an empty folder on the root, not a card).
fn volumes() -> Vec<Volume> {
    VOLUMES
        .iter()
        .filter(|(mount, _)| *mount == "/" || mounted(mount))
        .filter_map(|&(mount, name)| {
            let (used, total) = statvfs(mount)?;
            Some(Volume { name, used, total })
        })
        .collect()
}

/// Bytes used and in all on the file system at `path`.
fn statvfs(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: statvfs fills the struct passed and reads the NUL-terminated path, both valid
    // for the call; a zeroed statvfs is a valid value of a plain C struct.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    // The fields' widths differ by platform.
    fn wide(n: impl Into<u64>) -> u64 {
        n.into()
    }
    let frag = wide(st.f_frsize);
    let total = wide(st.f_blocks) * frag;
    let free = wide(st.f_bfree) * frag;
    (total > 0).then_some((total.saturating_sub(free), total))
}

/// Each connected DRM connector and its first (preferred) mode.
fn panels(drm: &Path) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir(drm)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let (_, connector) = name.split_once('-')?;
            if !name.starts_with("card") || text(e.path().join("status"))? != "connected" {
                return None;
            }
            let mode = text(e.path().join("modes"))?.lines().next()?.to_owned();
            Some((connector.to_owned(), mode))
        })
        .collect();
    found.sort();
    found
}

/// A size in kB as "123 MB" (whole MB).
pub fn mb(kb: u64) -> String {
    format!("{} MB", kb / 1024)
}

/// Two sizes in kB as "563 of 984 MB".
pub fn of(used: u64, total: u64) -> String {
    format!("{} of {} MB", used / 1024, total / 1024)
}

/// A size in bytes as "1.3 GB", or "640 MB" under a gigabyte.
pub fn bytes(n: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    if n >= 1000 * 1024 * 1024 {
        format!("{:.1} GB", n as f64 / GB)
    } else {
        format!("{} MB", n / (1024 * 1024))
    }
}

/// Two sizes in bytes as "1.3 of 1.6 GB", or "243 MB of 1.6 GB" when their units differ.
pub fn of_bytes(used: u64, total: u64) -> String {
    let (u, t) = (bytes(used), bytes(total));
    match (u.rsplit_once(' '), t.rsplit_once(' ')) {
        (Some((n, unit)), Some((_, same))) if unit == same => format!("{n} of {t}"),
        _ => format!("{u} of {t}"),
    }
}

/// `used` as a share of `total`, 0-100.
pub fn percent(used: u64, total: u64) -> i64 {
    if total == 0 {
        return 0;
    }
    (used.saturating_mul(100) / total).min(100) as i64
}

/// A span of seconds as "3 min", "2 h 5 min" or "1 d 3 h".
pub fn span(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) => format!("{h} h {m} min"),
        _ => format!("{d} d {h} h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &str = "MemTotal:        1008332 kB
MemFree:          228160 kB
MemAvailable:     431228 kB
Buffers:           18012 kB
Cached:           452524 kB
SwapCached:            0 kB
SwapTotal:       1008328 kB
SwapFree:        1008328 kB
Shmem:            216204 kB
CmaTotal:          32768 kB
CmaFree:           30592 kB
";

    #[test]
    fn reads_the_release() {
        let r = release(
            "NIBOS_VERSION=0.1.0-alpha2\nNIBOS_BUILD=b32321d\nNIBOS_BOARD=rg-ds-plus\n\
             NIBOS_EDITION=dslot\nNIBOS_IMAGE=rgds-dslot\nNIBOS_CHANNEL=stable\n",
        );
        assert_eq!(
            r,
            Release {
                version: "0.1.0-alpha2".into(),
                build: "b32321d".into(),
                board: "rg-ds-plus".into(),
                edition: "dslot".into(),
                channel: "stable".into(),
            }
        );
    }

    #[test]
    fn reads_meminfo() {
        let m = memory(MEMINFO).unwrap();
        assert_eq!(m.total, 1_008_332);
        assert_eq!(m.available, 431_228);
        assert_eq!(m.used(), 577_104);
        assert_eq!(m.shared, 216_204);
        assert_eq!(m.cache, 452_524 + 18_012 - 216_204);
        assert_eq!((m.swap_total, m.swap_used), (1_008_328, 0));
        assert_eq!(m.cma, Some((2176, 32768)));
        assert!(memory("MemFree: 1 kB\n").is_none());
    }

    #[test]
    fn reads_the_rest() {
        assert_eq!(rss("Name:\tdslot\nVmRSS:\t  127564 kB\n"), Some(127_564));
        assert_eq!(
            load("0.59 0.70 0.48 1/177 4518\n"),
            Some([0.59, 0.70, 0.48])
        );
        assert_eq!(uptime("918.37 3480.35\n"), Some(918));
        assert_eq!(
            swap("Filename\t\tType\t\tSize\t\tUsed\n/dev/zram0  partition\t1008328\t0\t100\n"),
            Some("zram0".into())
        );
        assert_eq!(swap("Filename\n"), None);
        assert_eq!(
            root_label("root=PARTLABEL=nibos-root-b rootfstype=ext4 rw quiet"),
            Some("nibos-root-b".into())
        );
        assert_eq!(root_label("root=/dev/mmcblk0p12 rw"), None);
        let info = Info {
            root: Some("nibos-root-b".into()),
            ..Info::default()
        };
        assert_eq!(info.slot(), Some("B"));
    }

    #[test]
    fn shows_sizes() {
        assert_eq!(mb(1_008_332), "984 MB");
        assert_eq!(of(577_104, 1_008_332), "563 of 984 MB");
        assert_eq!(bytes(1_400_942_592), "1.3 GB");
        assert_eq!(bytes(255_709_184), "243 MB");
        assert_eq!(of_bytes(1_400_942_592, 1_767_247_872), "1.3 of 1.6 GB");
        assert_eq!(of_bytes(255_709_184, 1_767_247_872), "243 MB of 1.6 GB");
        assert_eq!(percent(577_104, 1_008_332), 57);
        assert_eq!(percent(1, 0), 0);
        assert_eq!(span(918), "15 min");
        assert_eq!(span(7_500), "2 h 5 min");
        assert_eq!(span(97_200), "1 d 3 h");
    }

    #[test]
    fn summary_says() {
        let mut info = Info::default();
        assert_eq!(info.summary(), "Reading\u{2026}");
        info.read = true;
        assert_eq!(info.summary(), "This device");
        info.board = Some("Anbernic RG DS+".into());
        info.release = Some(Release {
            version: "0.1.0-alpha2".into(),
            ..Release::default()
        });
        assert_eq!(info.summary(), "nibOS 0.1.0-alpha2 on Anbernic RG DS+");
    }

    /// Reading the live system never panics, whatever the machine has.
    #[test]
    fn reads_without_fuss() {
        let info = read();
        assert!(info.read);
    }
}
