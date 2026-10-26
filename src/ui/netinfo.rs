//! The Wi-Fi page's **Network info** page (nibOS): what a player reads out when asked "are you
//! on Wi-Fi?". Read-only: whether Wi-Fi is on and joined to a network, the network and its
//! signal, the IPv4 address, gateway and DNS servers, the device's name on the network
//! (`<hostname>.local`, what to ssh to), the Wi-Fi MAC, and whether the internet, nibOS's update
//! server and its bug tracker can be reached.
//!
//! Nothing here needs root or a daemon of its own. The facts come from nibkit (`nib wifi
//! status` through `nibkit::device::wifi`) and from the system as any user may read it: `ip -j
//! addr` and `ip -j route`, iwd's `iwctl station <if> show` (or `iw dev <if> link` where iwctl
//! isn't there), systemd-resolved's `/run/systemd/resolve/resolv.conf`, `/etc/hostname`, and
//! the bug tracker's host alone out of `/etc/nibos/bug-report.conf`'s DSN (never its key). A
//! [`Watch`] reads them on a thread every couple of seconds while the Wi-Fi page is up; the
//! reachability checks run on a thread of their own when the page opens and on demand.
//!
//! On the stock OS there is no Wi-Fi page and [`Watch::start`] gives `None`.

use std::fmt;

/// How a host answered a reachability check.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(feature = "nibos"), allow(dead_code))] // The stock OS never checks.
pub enum Reach {
    /// Not checked yet.
    #[default]
    NotYet,
    Checking,
    /// Its name resolved and its HTTPS port answered.
    Reachable,
    /// Its name resolved (or it needed none) but nothing answered: no network, or no way out.
    NoRoute,
    /// Its name didn't resolve while an address did answer: the network is up, DNS isn't.
    DnsFailed,
}

impl fmt::Display for Reach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Reach::NotYet => "\u{2013}",
            Reach::Checking => "Checking\u{2026}",
            Reach::Reachable => "Reachable",
            Reach::NoRoute => "No route",
            Reach::DnsFailed => "DNS failed",
        })
    }
}

/// The three hosts checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Checks {
    /// A well-known host on the internet.
    pub internet: Reach,
    /// nibOS's update server.
    pub updates: Reach,
    /// nibOS's bug tracker, where one is set.
    pub tracker: Reach,
}

/// What iwd says about the station (the Wi-Fi interface).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Station {
    /// iwd's state: "connected", "disconnected", "connecting", ...
    pub state: String,
    /// The network joined, by name.
    pub network: Option<String>,
    /// Its signal, in dBm (negative; nearer 0 is stronger).
    pub rssi: Option<i32>,
    /// Its channel's frequency, in MHz.
    pub frequency: Option<u32>,
    /// "WPA2-Personal", "Open", ...
    pub security: Option<String>,
}

impl Station {
    /// Whether it is joined to a network.
    pub fn connected(&self) -> bool {
        self.state == "connected"
    }

    /// The signal in bars of 4, as the status bar shows it; 0 without one.
    pub fn bars(&self) -> u8 {
        match self.rssi {
            Some(dbm) if dbm >= -60 => 4,
            Some(dbm) if dbm >= -67 => 3,
            Some(dbm) if dbm >= -75 => 2,
            Some(_) => 1,
            None => 0,
        }
    }

    /// The band, from the frequency: "2.4 GHz", "5 GHz", "6 GHz".
    pub fn band(&self) -> Option<&'static str> {
        Some(match self.frequency? {
            2400..=2500 => "2.4 GHz",
            5000..=5900 => "5 GHz",
            5925..=7125 => "6 GHz",
            _ => return None,
        })
    }
}

/// The network as last read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// Read at least once: before that every field is a blank, not a fact.
    pub read: bool,
    /// Wi-Fi's switch: on, off, or unknown (no `nib`).
    pub wifi: Option<bool>,
    /// The Wi-Fi interface's name ("wlan0"), where there is one.
    pub interface: Option<String>,
    /// iwd's view of it; `None` when it can't be asked (the radio blocked, no iwd).
    pub station: Option<Station>,
    /// The IPv4 address and its prefix length.
    pub address: Option<(String, u8)>,
    /// The default route's gateway.
    pub gateway: Option<String>,
    /// The DNS servers, as resolved uses them.
    pub dns: Vec<String>,
    /// The device's name on the network: "nibos.local".
    pub name: String,
    /// The Wi-Fi interface's MAC address.
    pub mac: Option<String>,
    /// The bug tracker's host, from the DSN; `None` where no tracker is set.
    pub tracker: Option<String>,
    pub checks: Checks,
}

impl Info {
    /// Whether it is on and joined to a network, as the page says in one line.
    pub fn summary(&self) -> String {
        if !self.read {
            return "Network info".into();
        }
        match (self.wifi, &self.station) {
            (Some(false), _) => "Wi-Fi is off".into(),
            (_, Some(s)) if s.connected() => match &s.network {
                Some(name) => format!("Connected to {name}"),
                None => "Connected".into(),
            },
            (_, Some(s)) if s.state == "connecting" => "Joining a network\u{2026}".into(),
            (Some(true), _) | (None, Some(_)) => "Wi-Fi is on, not connected".into(),
            (None, None) => "No Wi-Fi".into(),
        }
    }

    /// The address with its prefix, "192.168.68.77 /22".
    pub fn address_line(&self) -> Option<String> {
        self.address
            .as_ref()
            .map(|(ip, prefix)| format!("{ip} /{prefix}"))
    }
}

/// nibOS's update server (`nib update` asks GitHub's API for releases).
pub const UPDATE_HOST: &str = "api.github.com";

#[cfg(feature = "nibos")]
pub use real::Watch;
#[cfg(not(feature = "nibos"))]
pub use stub::Watch;

#[cfg(not(feature = "nibos"))]
mod stub {
    //! The stock OS: no Wi-Fi page, so nothing to watch.
    #![allow(dead_code)]
    use super::Info;

    pub struct Watch;

    impl Watch {
        pub fn start() -> Option<Watch> {
            None
        }
        pub fn now(&self) -> Info {
            Info::default()
        }
        pub fn check_again(&self) {}
    }
}

#[cfg(feature = "nibos")]
mod real {
    use super::{Checks, Info, Reach, Station, UPDATE_HOST};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::path::Path;
    use std::process::Command;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    /// How often the facts are read again while the page is up.
    const EVERY: Duration = Duration::from_secs(2);
    /// How long a host gets to answer on its HTTPS port.
    const CONNECT: Duration = Duration::from_secs(4);
    const RESOLV: &str = "/run/systemd/resolve/resolv.conf";
    const HOSTNAME: &str = "/etc/hostname";
    const TRACKER_CONF: &str = "/etc/nibos/bug-report.conf";
    /// The well-known host that stands for "the internet", and its address for telling a dead
    /// network from dead DNS.
    const INTERNET_HOST: &str = "one.one.one.one";
    const INTERNET_IP: &str = "1.1.1.1";

    struct Shared {
        info: Mutex<Info>,
        /// Set to stop the thread; the condvar wakes it.
        stop: Mutex<bool>,
        wake: Condvar,
        /// A check thread is running: another isn't started until it is done.
        checking: Mutex<bool>,
    }

    /// The facts, read on a thread every couple of seconds for as long as it is held.
    pub struct Watch {
        shared: Arc<Shared>,
    }

    impl Watch {
        /// Starts reading; `None` only if the thread can't be started. (Where the commands
        /// aren't there, a desktop preview, the lines stay blank.)
        pub fn start() -> Option<Watch> {
            let shared = Arc::new(Shared {
                info: Mutex::new(Info::default()),
                stop: Mutex::new(false),
                wake: Condvar::new(),
                checking: Mutex::new(false),
            });
            let worker = shared.clone();
            std::thread::Builder::new()
                .name("netinfo".into())
                .spawn(move || loop {
                    let found = read();
                    {
                        let mut info = lock(&worker.info);
                        let checks = info.checks;
                        *info = found;
                        info.checks = checks;
                    }
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
            let watch = Watch { shared };
            watch.check_again();
            Some(watch)
        }

        /// The facts as last read.
        pub fn now(&self) -> Info {
            lock(&self.shared.info).clone()
        }

        /// Runs the reachability checks again, on a thread; the rows say "Checking…" until each
        /// answers. Nothing happens while a run is still going.
        pub fn check_again(&self) {
            {
                let mut checking = lock(&self.shared.checking);
                if *checking {
                    return;
                }
                *checking = true;
            }
            let shared = self.shared.clone();
            lock(&shared.info).checks = Checks {
                internet: Reach::Checking,
                updates: Reach::Checking,
                tracker: Reach::Checking,
            };
            let _ = std::thread::Builder::new()
                .name("netcheck".into())
                .spawn(move || {
                    let internet = check(INTERNET_HOST, Some(INTERNET_IP));
                    lock(&shared.info).checks.internet = internet;
                    // With no way out at all, the others can't do better than the internet.
                    let onwards = |host: &str| match internet {
                        Reach::NoRoute => Reach::NoRoute,
                        _ => check(host, None),
                    };
                    let updates = onwards(UPDATE_HOST);
                    lock(&shared.info).checks.updates = updates;
                    let tracker = match tracker() {
                        Some(host) => onwards(&host),
                        None => Reach::NotYet,
                    };
                    lock(&shared.info).checks.tracker = tracker;
                    *lock(&shared.checking) = false;
                });
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

    /// A command's standard output, if it ran and succeeded.
    fn run(words: &[&str]) -> Option<String> {
        let out = Command::new(words[0]).args(&words[1..]).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Everything but the checks, read from the system now.
    fn read() -> Info {
        let addr = run(&["ip", "-j", "addr"]).unwrap_or_default();
        let interface = wireless_interface(&addr).unwrap_or_else(|| "wlan0".into());
        let link = parse_addr(&addr, &interface);
        let station = run(&["iwctl", "station", &interface, "show"])
            .and_then(|out| parse_iwctl(&out))
            .or_else(|| {
                run(&["iw", "dev", &interface, "link"]).and_then(|out| parse_iw_link(&out))
            });
        let gateway = run(&["ip", "-j", "route"]).and_then(|routes| parse_gateway(&routes));
        let dns = std::fs::read_to_string(RESOLV)
            .map(|text| parse_resolv(&text))
            .unwrap_or_default();
        let name = std::fs::read_to_string(HOSTNAME)
            .ok()
            .map(|text| local_name(&text))
            .unwrap_or_else(|| "nibos.local".into());
        let mac = link.as_ref().and_then(|l| l.mac.clone()).or_else(|| {
            let path = format!("/sys/class/net/{interface}/address");
            let text = std::fs::read_to_string(path).ok()?;
            Some(text.trim().to_owned())
        });
        Info {
            read: true,
            wifi: nibkit::device::wifi(),
            interface: Some(interface),
            station,
            address: link.and_then(|l| l.ipv4),
            gateway,
            dns,
            name,
            mac,
            tracker: tracker(),
            checks: Checks::default(),
        }
    }

    /// The bug tracker's host, out of the DSN in nibOS's conf.
    fn tracker() -> Option<String> {
        let text = std::fs::read_to_string(Path::new(TRACKER_CONF)).ok()?;
        tracker_host(&text)
    }

    /// Whether `host` answers on its HTTPS port. With `literal`, an address to try when the
    /// name doesn't resolve, to tell DNS failing from the network being down.
    fn check(host: &str, literal: Option<&str>) -> Reach {
        match (host, 443).to_socket_addrs() {
            Ok(addrs) => {
                // IPv4 first: the DS+'s networks are, and a v6 default route may lead nowhere.
                let mut addrs: Vec<_> = addrs.collect();
                addrs.sort_by_key(|a| a.is_ipv6());
                if addrs
                    .iter()
                    .take(2)
                    .any(|a| TcpStream::connect_timeout(a, CONNECT).is_ok())
                {
                    Reach::Reachable
                } else {
                    Reach::NoRoute
                }
            }
            Err(_) => match literal.and_then(|ip| (ip, 443).to_socket_addrs().ok()) {
                Some(mut addrs) => match addrs.next() {
                    Some(a) if TcpStream::connect_timeout(&a, CONNECT).is_ok() => Reach::DnsFailed,
                    _ => Reach::NoRoute,
                },
                None => Reach::DnsFailed,
            },
        }
    }

    /// An interface as `ip -j addr` lists it: its MAC and IPv4 address.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Link {
        pub mac: Option<String>,
        pub ipv4: Option<(String, u8)>,
    }

    /// The first wireless interface in `ip -j addr`'s output ("wl..."), by name.
    pub fn wireless_interface(json: &str) -> Option<String> {
        let links: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
        links.iter().find_map(|l| {
            let name = l.get("ifname")?.as_str()?;
            name.starts_with("wl").then(|| name.to_owned())
        })
    }

    /// The interface `ifname` in `ip -j addr`'s output: its MAC and its first global IPv4 address.
    pub fn parse_addr(json: &str, ifname: &str) -> Option<Link> {
        let links: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
        let link = links
            .iter()
            .find(|l| l.get("ifname").and_then(|n| n.as_str()) == Some(ifname))?;
        let mac = link
            .get("address")
            .and_then(|a| a.as_str())
            .filter(|a| !a.is_empty())
            .map(str::to_owned);
        let ipv4 = link
            .get("addr_info")
            .and_then(|a| a.as_array())
            .and_then(|addrs| {
                addrs.iter().find_map(|a| {
                    (a.get("family")?.as_str()? == "inet").then_some(())?;
                    let ip = a.get("local")?.as_str()?.to_owned();
                    let prefix = a.get("prefixlen")?.as_u64()? as u8;
                    Some((ip, prefix))
                })
            });
        Some(Link { mac, ipv4 })
    }

    /// The IPv4 default route's gateway in `ip -j route`'s output.
    pub fn parse_gateway(json: &str) -> Option<String> {
        let routes: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
        routes.iter().find_map(|r| {
            (r.get("dst")?.as_str()? == "default").then_some(())?;
            Some(r.get("gateway")?.as_str()?.to_owned())
        })
    }

    /// The name servers in a resolv.conf (resolved's uplink one, `/run/systemd/resolve/resolv.conf`;
    /// the stub's own 127.0.0.53 is left out should `/etc/resolv.conf` be read instead). With
    /// no link up it lists resolved's fallback servers (9.9.9.9, ...), so the page shows the
    /// DNS line only while there is a route.
    pub fn parse_resolv(text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next()? == "nameserver").then_some(())?;
                let server = words.next()?;
                (!server.starts_with("127.0.0.53")).then(|| server.to_owned())
            })
            .collect()
    }

    /// `iwctl station <if> show`'s table: it colours its output and pads its columns.
    pub fn parse_iwctl(text: &str) -> Option<Station> {
        let plain = strip_ansi(text);
        let mut station = Station::default();
        let mut seen = false;
        for line in plain.lines() {
            let line = line.trim();
            // The columns are two or more spaces apart; the names may hold one space.
            let mut cells = line.split("  ").map(str::trim).filter(|c| !c.is_empty());
            let (Some(key), Some(value)) = (cells.next(), cells.next()) else {
                continue;
            };
            if key.starts_with('-') || key == "Settable" {
                continue;
            }
            match key {
                "State" => (station.state, seen) = (value.to_owned(), true),
                "Connected network" => station.network = Some(value.to_owned()),
                "RSSI" => station.rssi = value.strip_suffix(" dBm").and_then(|v| v.parse().ok()),
                "Frequency" => station.frequency = value.parse().ok(),
                "Security" => station.security = Some(value.to_owned()),
                _ => {}
            }
        }
        seen.then_some(station)
    }

    /// `iw dev <if> link`: "Not connected.", or the BSS, SSID, freq and signal lines.
    pub fn parse_iw_link(text: &str) -> Option<Station> {
        let mut station = Station::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("Not connected") {
                station.state = "disconnected".into();
            } else if line.starts_with("Connected to ") {
                station.state = "connected".into();
            } else if let Some(ssid) = line.strip_prefix("SSID: ") {
                station.network = Some(ssid.to_owned());
            } else if let Some(freq) = line.strip_prefix("freq: ") {
                station.frequency = freq.split('.').next().and_then(|f| f.parse().ok());
            } else if let Some(signal) = line.strip_prefix("signal: ") {
                station.rssi = signal.strip_suffix(" dBm").and_then(|v| v.parse().ok());
            }
        }
        (!station.state.is_empty()).then_some(station)
    }

    /// `text` without its terminal colour codes (ESC `[` ... letter).
    fn strip_ansi(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                if chars.next() == Some('[') {
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    /// The bug tracker's host alone, out of a bug-report.conf's `DSN=scheme://KEY@HOST[:PORT]/...`
    /// line (the file is shell: the value may be quoted). `None` without a DSN. The key is never
    /// part of the answer.
    pub fn tracker_host(conf: &str) -> Option<String> {
        let dsn = conf.lines().find_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let value = line.strip_prefix("DSN=")?.trim();
            let value = value.trim_matches(|c| c == '"' || c == '\'');
            (!value.is_empty()).then(|| value.to_owned())
        })?;
        let rest = dsn.split_once("://").map_or(dsn.as_str(), |(_, r)| r);
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        let hostport = rest.split('/').next().unwrap_or(rest);
        let host = hostport.split(':').next().unwrap_or(hostport);
        (!host.is_empty()).then(|| host.to_owned())
    }

    /// `/etc/hostname`'s name as mDNS gives it out: "nibos.local".
    pub fn local_name(text: &str) -> String {
        let name = text.lines().next().unwrap_or("").trim();
        let name = if name.is_empty() { "nibos" } else { name };
        format!("{name}.local")
    }
}

#[cfg(all(test, feature = "nibos"))]
mod tests {
    use super::real::*;
    use super::*;

    // Captured on the DS+ (nibOS, iwd 3.x, iproute2 6.x), 2026-10-27.
    const ADDR: &str = r#"[{"ifindex":1,"ifname":"lo","flags":["LOOPBACK","UP","LOWER_UP"],"mtu":65536,"qdisc":"noqueue","operstate":"UNKNOWN","group":"default","txqlen":1000,"link_type":"loopback","address":"00:00:00:00:00:00","broadcast":"00:00:00:00:00:00","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8,"scope":"host","label":"lo","valid_life_time":4294967295,"preferred_life_time":4294967295},{"family":"inet6","local":"::1","prefixlen":128,"scope":"host","noprefixroute":true,"valid_life_time":4294967295,"preferred_life_time":4294967295}]},{"ifindex":2,"ifname":"wlan0","flags":["BROADCAST","MULTICAST","UP","LOWER_UP"],"mtu":1500,"qdisc":"noqueue","operstate":"UP","group":"default","txqlen":1000,"link_type":"ether","address":"b4:65:dc:72:e7:7d","broadcast":"ff:ff:ff:ff:ff:ff","addr_info":[{"family":"inet","local":"192.168.68.77","prefixlen":22,"scope":"global","dynamic":true,"noprefixroute":true,"label":"wlan0","valid_life_time":7029,"preferred_life_time":7029},{"family":"inet6","local":"fe80::b665:dcff:fe72:e77d","prefixlen":64,"scope":"link","protocol":"kernel_ll","valid_life_time":4294967295,"preferred_life_time":4294967295}]}]"#;
    // The same interface with Wi-Fi off: the link is down and has no address.
    const ADDR_DOWN: &str = r#"[{"ifindex":1,"ifname":"lo","flags":["LOOPBACK","UP","LOWER_UP"],"mtu":65536,"address":"00:00:00:00:00:00","addr_info":[{"family":"inet","local":"127.0.0.1","prefixlen":8,"scope":"host","label":"lo"}]},{"ifindex":2,"ifname":"wlan0","flags":["BROADCAST","MULTICAST"],"mtu":1500,"qdisc":"noqueue","operstate":"DOWN","group":"default","txqlen":1000,"link_type":"ether","address":"b4:65:dc:72:e7:7d","broadcast":"ff:ff:ff:ff:ff:ff","addr_info":[]}]"#;
    const ROUTE: &str = r#"[{"dst":"default","gateway":"192.168.68.1","dev":"wlan0","protocol":"dhcp","prefsrc":"192.168.68.77","metric":302,"flags":[]},{"dst":"192.168.68.0/22","dev":"wlan0","protocol":"dhcp","scope":"link","metric":302,"flags":[]}]"#;
    const RESOLV: &str = "# This is /run/systemd/resolve/resolv.conf managed by man:systemd-resolved(8).\n# Do not edit.\n\nnameserver 192.168.64.1\nnameserver 192.168.68.1\nsearch .\n";
    const IWCTL: &str = "                                 Station: wlan0\u{1b}[1;90m                                \u{1b}[0m\n\u{1b}[90m--------------------------------------------------------------------------------\n\u{1b}[0m\u{1b}[1;90m  Settable  Property              Value                                          \n\u{1b}[0m\u{1b}[90m--------------------------------------------------------------------------------\n\u{1b}[0m            Scanning              no                                               \n            State                 connected                                        \n            Connected network     Endless                                          \n            IPv4 address          192.168.68.77                                    \n            ConnectedBss          78:8c:b5:7d:76:73                                \n            Frequency             5200                                             \n            Channel               40                                               \n            Security              WPA2-Personal                                    \n            RSSI                  -48 dBm                                          \n            AverageRSSI           -44 dBm                                          \n            RxMode                802.11ac                                         \n            TxBitrate             351000 Kbit/s                                    \n            ConnectedTime         174 s                                            \n\n";
    const IWCTL_OFF: &str = "                                 Station: wlan0\u{1b}[1;90m                                \u{1b}[0m\n\u{1b}[90m--------------------------------------------------------------------------------\n\u{1b}[0m\u{1b}[1;90m  Settable  Property              Value                                          \n\u{1b}[0m\u{1b}[90m--------------------------------------------------------------------------------\n\u{1b}[0m            Scanning              no                                               \n            State                 disconnected                                     \n\n";
    const IW: &str = "Connected to 78:8c:b5:7d:76:73 (on wlan0)\n\tSSID: Endless\n\tfreq: 5200.0\n\tRX: 1094102 bytes (4888 packets)\n\tTX: 38433 bytes (172 packets)\n\tsignal: -48 dBm\n\trx bitrate: 135.0 MBit/s VHT-MCS 7 40MHz VHT-NSS 1\n\tbss flags: short-slot-time\n";

    #[test]
    fn the_wireless_interface_and_its_addresses() {
        assert_eq!(wireless_interface(ADDR).as_deref(), Some("wlan0"));
        let link = parse_addr(ADDR, "wlan0").unwrap();
        assert_eq!(link.mac.as_deref(), Some("b4:65:dc:72:e7:7d"));
        assert_eq!(link.ipv4, Some(("192.168.68.77".to_owned(), 22)));
        assert_eq!(parse_addr(ADDR, "eth0"), None);
        let down = parse_addr(ADDR_DOWN, "wlan0").unwrap();
        assert_eq!(down.mac.as_deref(), Some("b4:65:dc:72:e7:7d"));
        assert_eq!(down.ipv4, None);
        assert_eq!(parse_addr("not json", "wlan0"), None);
        assert_eq!(wireless_interface("[]"), None);
    }

    #[test]
    fn the_default_gateway() {
        assert_eq!(parse_gateway(ROUTE).as_deref(), Some("192.168.68.1"));
        assert_eq!(parse_gateway("[]"), None);
        assert_eq!(
            parse_gateway(r#"[{"dst":"192.168.68.0/22","dev":"wlan0"}]"#),
            None
        );
    }

    #[test]
    fn resolveds_name_servers() {
        assert_eq!(parse_resolv(RESOLV), ["192.168.64.1", "192.168.68.1"]);
        assert_eq!(
            parse_resolv("nameserver 127.0.0.53\noptions edns0 trust-ad\n"),
            Vec::<String>::new()
        );
        assert!(parse_resolv("").is_empty());
    }

    #[test]
    fn iwctls_table() {
        let s = parse_iwctl(IWCTL).unwrap();
        assert_eq!(s.state, "connected");
        assert!(s.connected());
        assert_eq!(s.network.as_deref(), Some("Endless"));
        assert_eq!(s.rssi, Some(-48));
        assert_eq!(s.frequency, Some(5200));
        assert_eq!(s.security.as_deref(), Some("WPA2-Personal"));
        assert_eq!(s.bars(), 4);
        assert_eq!(s.band(), Some("5 GHz"));
        let off = parse_iwctl(IWCTL_OFF).unwrap();
        assert_eq!(off.state, "disconnected");
        assert_eq!(off.network, None);
        assert_eq!(off.bars(), 0);
        assert_eq!(parse_iwctl("Device wlan0 not found\n"), None);
        assert_eq!(parse_iwctl(""), None);
    }

    #[test]
    fn iws_link() {
        let s = parse_iw_link(IW).unwrap();
        assert_eq!(s.state, "connected");
        assert_eq!(s.network.as_deref(), Some("Endless"));
        assert_eq!(s.rssi, Some(-48));
        assert_eq!(s.frequency, Some(5200));
        let off = parse_iw_link("Not connected.\n").unwrap();
        assert_eq!(off.state, "disconnected");
        assert_eq!(
            parse_iw_link("command failed: No such device (-19)\n"),
            None
        );
    }

    #[test]
    fn signal_bars() {
        let at = |rssi| {
            Station {
                rssi: Some(rssi),
                ..Station::default()
            }
            .bars()
        };
        assert_eq!(
            (
                at(-40),
                at(-60),
                at(-61),
                at(-67),
                at(-70),
                at(-75),
                at(-80)
            ),
            (4, 4, 3, 3, 2, 2, 1)
        );
    }

    #[test]
    fn the_trackers_host_without_its_key() {
        let conf = "# nib bug-report: the bug tracker a report is sent to (a Sentry-style DSN). Empty: saved on the card only.\nDSN=https://0123456789abcdef0123456789abcdef@tracker.example.org/3\n";
        assert_eq!(tracker_host(conf).as_deref(), Some("tracker.example.org"));
        assert_eq!(
            tracker_host("DSN=\"https://key@glitch.example.net:8443/api/7\"\n").as_deref(),
            Some("glitch.example.net")
        );
        assert_eq!(tracker_host("# a comment\nDSN=\n"), None);
        assert_eq!(tracker_host(""), None);
        assert_eq!(tracker_host("# DSN=https://key@commented.out/1\n"), None);
        // The key stays out of whatever is shown.
        assert!(!tracker_host(conf).unwrap().contains("0123456789abcdef"));
    }

    #[test]
    fn the_name_on_the_network() {
        assert_eq!(local_name("nibos\n"), "nibos.local");
        assert_eq!(local_name(""), "nibos.local");
        assert_eq!(local_name("  rgds  \n"), "rgds.local");
    }

    #[test]
    fn the_summary_line() {
        let mut info = Info::default();
        assert_eq!(info.summary(), "Network info");
        info.read = true;
        info.wifi = Some(false);
        assert_eq!(info.summary(), "Wi-Fi is off");
        info.wifi = Some(true);
        assert_eq!(info.summary(), "Wi-Fi is on, not connected");
        info.station = parse_iwctl(IWCTL_OFF);
        assert_eq!(info.summary(), "Wi-Fi is on, not connected");
        info.station = parse_iwctl(IWCTL);
        assert_eq!(info.summary(), "Connected to Endless");
        info.address = Some(("192.168.68.77".into(), 22));
        assert_eq!(info.address_line().as_deref(), Some("192.168.68.77 /22"));
        assert_eq!(Reach::DnsFailed.to_string(), "DNS failed");
    }
}
