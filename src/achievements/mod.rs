//! RetroAchievements: the player's sign-in, kept for the emulators to use.
//!
//! DSlot signs in once, with the username and password typed on its keyboard, through the
//! service's own `dorequest.php` (`login2`, as rcheevos builds the request: nibkit's
//! `nib-cheevos`, which every emulator here uses too), and keeps what the service answers with: the username as the service spells it and a **token**, which signs the
//! player in from then on without the password. The password is sent once and never written
//! anywhere. The token is kept in a file of its own (`retroachievements.token`, beside
//! settings.toml, readable by DSlot's user only), two lines, `username` then `token`: the
//! format DSperate's `--cheevos-token` reads (nib-cheevos' `launch` reads and writes it). An
//! emulator gets it through its environment when a game starts ([`Ra::env_for`]):
//! `RA_TOKEN_FILE`, `RA_USERNAME` and `RA_HARDCORE`; never the password, never a word of it on
//! the command line; and only an emulator whose description says it earns achievements for the
//! game's system (`[achievements]`, docs/emulators.md).
//!
//! The requests go through `curl`, on a thread, as nibOS's own tools do (`nib update`,
//! `nib bug-report`): DSlot has no TLS of its own. Where there is no `curl` the page isn't
//! offered ([`available`]).
//!
//! The service has no way to make an account from a device: its public API reads data only,
//! and rcheevos' endpoints sign in, start sessions and award. So the page shows the sign-up
//! page's address and a QR code of it, for a phone ([`SIGN_UP`], [`qr`]).
//!
//! - `service`: the requests (nib-cheevos' `api`), and what DSlot makes of the answers.
//! - `set`: a game's set and the player's unlocks, kept on disk per player, by the ROM's hash
//!   (nib-cheevos' `hash`, rcheevos' rhash: the identity the service gives a ROM, for every
//!   console).
//! - `lookup`: the shelf's worker, looking games up off the shelf's thread.
//!
//! [`Ra`] holds where the sign-in and the lookups are kept; main makes it and hands it down.

pub mod lookup;
pub mod service;
pub mod set;

pub use service::Error;

use crate::emulator::Emulator;
use crate::rom::Console;
use nib_cheevos::api::Secret;
use nib_cheevos::launch::{self, Credentials};
use set::Store;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// Where an account is made, on a phone or a computer.
pub use nib_cheevos::paths::SIGN_UP;

/// Whether hardcore is offered: the Mode row on the Achievements page, and `RA_HARDCORE=1`
/// for a game. Off until the service approves our emulators for hardcore unlocks
/// (docs/retroachievements.md); until then every game plays softcore, whatever settings.toml
/// says ([`Ra::env`]).
pub const HARDCORE_OFFERED: bool = false;
/// Whether the service's identity for the console's games can be had here (nib-cheevos'
/// `hash`): the shelf looks those games up. Not Game & Watch, which rcheevos has no rule for.
pub fn hashed(console: Console) -> bool {
    console != Console::GameWatch && nib_cheevos::hash::console(console.folder()).is_some()
}

/// Whether the page is offered: `curl` is there to talk to the service. Looked for along
/// `PATH` once a run.
pub fn available() -> bool {
    service::available()
}

/// RetroAchievements as DSlot keeps it: where the sign-in is kept (the token file), and
/// where lookups keep what they found. Made once, in main, from the platform's folders, and
/// handed to whatever needs either.
#[derive(Clone, Debug)]
pub struct Ra {
    /// `retroachievements.token`, beside settings.toml (platform::Dirs::achievements).
    pub token: PathBuf,
    pub store: Store,
}

/// What a sign-in, or a check of the one kept, came to: the sign-in kept and the player's
/// standing, or why not.
pub type SignedIn = Result<(Account, Standing), Error>;

impl Ra {
    /// The token file at `token`, the lookups under DSlot's cache folder `cache`.
    pub fn new(token: PathBuf, cache: &Path) -> Ra {
        Ra {
            token,
            store: Store::new(cache),
        }
    }

    /// Who is signed in, read from the token file now.
    pub fn account(&self) -> Option<Account> {
        Account::load(&self.token)
    }

    /// Signs in with a password: the service answers with the token, which is kept before the
    /// receiver hears of it, on the request's own thread, so that a sign-in stands even if
    /// the menu that asked has gone. The password goes to `curl` on its standard input, never
    /// in an argument, and is written nowhere.
    pub fn sign_in(&self, user: &str, password: &str) -> Receiver<SignedIn> {
        let (user, password) = (user.to_owned(), password.to_owned());
        let token = self.token.clone();
        request(move || keep_sign_in(&token, service::login(&user, Secret::Password(&password))))
    }

    /// Signs in again with the token kept: whether it still stands, and the standing now. A
    /// token the service refuses is dropped (on the request's thread, as a sign-in is kept),
    /// unless the file has been changed since.
    pub fn check(&self, account: Account) -> Receiver<SignedIn> {
        let token = self.token.clone();
        request(move || {
            let answer = service::login(&account.user, Secret::Token(&account.token));
            checked(&token, &account, answer)
        })
    }

    /// Signs out: the token file is removed.
    pub fn sign_out(&self) {
        match Account::clear(&self.token) {
            Ok(()) => eprintln!("dslot: RetroAchievements: signed out"),
            Err(e) => eprintln!("dslot: cannot remove {}: {e}", self.token.display()),
        }
    }

    /// The game at `path` was just played: what is kept of it for the player signed in is
    /// asked about again. From the hashes index alone, never a read of the ROM: this is on
    /// the main thread, between the game and the shelf.
    pub fn played(&self, path: &Path) {
        if let Some(account) = self.account() {
            self.store.played(&account.user, path);
        }
    }

    /// The environment a game starts with when the player is signed in: where the token file
    /// is, who it signs in as, and `RA_HARDCORE`: `1` when hardcore was `chosen` (settings.toml)
    /// and is offered at all ([`HARDCORE_OFFERED`]), else `0`.
    pub fn env(&self, chosen: bool) -> Vec<(String, String)> {
        let Some(account) = self.account() else {
            return Vec::new();
        };
        let hardcore = chosen && HARDCORE_OFFERED;
        launch::env(&self.token, &account.user, hardcore)
    }

    /// The environment a game of `system` starts with in `emulator`: [`Ra::env`] when the
    /// emulator's description says it earns achievements for the system (`[achievements]`),
    /// hardcore only where it says it may play it; else nothing, not even the sign-in.
    pub fn env_for(
        &self,
        emulator: &Emulator,
        system: &str,
        chosen: bool,
    ) -> Vec<(String, String)> {
        if !emulator.earns(system) {
            return Vec::new();
        }
        self.env(chosen && emulator.achievements.hardcore)
    }
}

/// Runs `work` on a thread; what it comes to arrives on the receiver.
fn request(work: impl FnOnce() -> SignedIn + Send + 'static) -> Receiver<SignedIn> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx
}

/// A sign-in's answer, kept: the token written to `path` before anyone is told. A sign-in the
/// file can't take is no sign-in.
fn keep_sign_in(path: &Path, answer: Result<(Standing, String), Error>) -> SignedIn {
    let (standing, token) = answer?;
    let account = Account {
        user: standing.user.clone(),
        token,
    };
    account.save(path).map_err(|e| {
        eprintln!("dslot: cannot write {}: {e}", path.display());
        Error::Failed(format!("the sign-in couldn't be kept: {e}"))
    })?;
    eprintln!("dslot: RetroAchievements: signed in as {}", account.user);
    Ok((account, standing))
}

/// A check's answer: the token kept still stands (kept again if the service has given a new
/// one), or it was refused and is dropped. Either only while the file still holds `kept`: a
/// sign-out or another sign-in since is left alone.
fn checked(path: &Path, kept: &Account, answer: Result<(Standing, String), Error>) -> SignedIn {
    let unchanged = || Account::load(path).as_ref() == Some(kept);
    match answer {
        Ok((standing, token)) => {
            let account = Account {
                user: standing.user.clone(),
                token,
            };
            if account != *kept && unchanged() {
                if let Err(e) = account.save(path) {
                    eprintln!("dslot: cannot write {}: {e}", path.display());
                }
            }
            Ok((account, standing))
        }
        Err(Error::Refused(why)) => {
            eprintln!("dslot: RetroAchievements: the sign-in kept was refused: {why}");
            if unchanged() {
                if let Err(e) = Account::clear(path) {
                    eprintln!("dslot: cannot remove {}: {e}", path.display());
                }
            }
            Err(Error::Refused(why))
        }
        Err(e) => Err(e),
    }
}

/// The sign-in kept: who, and the token that stands for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub user: String,
    pub token: String,
}

impl Account {
    /// Reads the token file: `username` on the first line, the token on the second. None
    /// when there is no file, or it is not whole.
    pub fn load(path: &Path) -> Option<Account> {
        let c = launch::read_token_file(path).ok()?;
        (!c.username.is_empty()).then_some(Account {
            user: c.username,
            token: c.token,
        })
    }

    /// Writes the token file, readable by DSlot's user only, whole or not at all: to a file
    /// beside it first, then renamed over it, so that a crash never leaves half a sign-in.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let credentials = Credentials {
            username: self.user.clone(),
            token: self.token.clone(),
        };
        launch::write_token_file(path, &credentials)
    }

    /// Removes the token file: signed out.
    pub fn clear(path: &Path) -> std::io::Result<()> {
        launch::remove_token_file(path)
    }
}

/// What the service says of the player when they sign in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Standing {
    /// The username as the service spells it.
    pub user: String,
    /// Hardcore points, and softcore points.
    pub score: u64,
    pub softcore: u64,
    /// Unread messages on the site.
    pub messages: u64,
}

impl Standing {
    /// "1,234 points", "56 casual points", both, or "No points yet".
    pub fn points(&self) -> String {
        match (self.score, self.softcore) {
            (0, 0) => "No points yet".into(),
            (h, 0) => format!("{} points", grouped(h)),
            (0, s) => format!("{} casual points", grouped(s)),
            (h, s) => format!("{} points \u{b7} {} casual", grouped(h), grouped(s)),
        }
    }
}

/// A count in thousands: "1,234,567".
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A QR code, for the sign-up page's address: `size` modules a side, row by row, dark ones true.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qr {
    pub size: usize,
    pub modules: Vec<bool>,
}

impl Qr {
    pub fn dark(&self, x: usize, y: usize) -> bool {
        self.modules
            .get(y * self.size + x)
            .copied()
            .unwrap_or(false)
    }
}

/// `text` as a QR code, at medium error correction.
pub fn qr(text: &str) -> Option<Qr> {
    let code = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let size = code.size().max(0) as usize;
    let mut modules = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            modules.push(code.get_module(x as i32, y as i32));
        }
    }
    Some(Qr { size, modules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn standing(user: &str) -> Standing {
        Standing {
            user: user.into(),
            score: 1,
            ..Standing::default()
        }
    }

    #[test]
    fn points_read_well() {
        let s = |score, softcore| Standing {
            score,
            softcore,
            ..Standing::default()
        };
        assert_eq!(s(0, 0).points(), "No points yet");
        assert_eq!(s(1234567, 0).points(), "1,234,567 points");
        assert_eq!(s(0, 56).points(), "56 casual points");
        assert_eq!(s(1000, 999).points(), "1,000 points \u{b7} 999 casual");
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(100), "100");
        assert_eq!(grouped(1000), "1,000");
    }

    #[test]
    fn the_token_file_round_trips_and_is_private() {
        let dir = TempDir::new("ra-token");
        let path = dir.path().join("sub").join("retroachievements.token");
        assert_eq!(Account::load(&path), None);
        let account = Account {
            user: "Alan".into(),
            token: "t0k3n".into(),
        };
        account.save(&path).unwrap();
        assert_eq!(Account::load(&path), Some(account.clone()));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "Alan\nt0k3n\n",
            "DSperate's --cheevos-token format"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // Written beside it and renamed: nothing else is left in the folder.
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["retroachievements.token"]);
        // A half file is no sign-in.
        std::fs::write(&path, "Alan\n").unwrap();
        assert_eq!(Account::load(&path), None);
        Account::clear(&path).unwrap();
        Account::clear(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn a_game_gets_the_sign_in_and_hardcore_only_where_it_is_offered() {
        let dir = TempDir::new("ra-env");
        let ra = Ra::new(dir.path().join("retroachievements.token"), dir.path());
        assert!(ra.env(true).is_empty(), "signed out: nothing");
        Account {
            user: "Alan".into(),
            token: "t0k3n".into(),
        }
        .save(&ra.token)
        .unwrap();
        let vars = ra.env(true);
        assert_eq!(
            vars[0],
            (
                "RA_TOKEN_FILE".to_owned(),
                ra.token.to_string_lossy().into_owned()
            )
        );
        assert_eq!(vars[1], ("RA_USERNAME".to_owned(), "Alan".to_owned()));
        let hardcore = if HARDCORE_OFFERED { "1" } else { "0" };
        assert_eq!(vars[2], ("RA_HARDCORE".to_owned(), hardcore.to_owned()));
        assert_eq!(vars.len(), 3);
        assert_eq!(ra.env(false)[2].1, "0");
    }

    /// The sign-in goes only to an emulator that earns achievements for the game's system,
    /// and hardcore only where its description allows it and DSlot offers it.
    #[test]
    fn only_an_emulator_that_earns_them_gets_the_sign_in() {
        let dir = TempDir::new("ra-env-for");
        let ra = Ra::new(dir.path().join("retroachievements.token"), dir.path());
        Account {
            user: "Alan".into(),
            token: "t0k3n".into(),
        }
        .save(&ra.token)
        .unwrap();
        let described = |text: &str| {
            crate::emulator::Emulators::assume_installed([("e".to_owned(), text.to_owned())])
        };
        let both = "name = \"E\"\n[systems]\nGB = [\"/e\"]\nGBA = [\"/e\"]\n";
        let earns = described(&format!(
            "{both}[achievements]\nsystems = [\"GB\"]\nhardcore = true\n"
        ));
        let e = earns.get("e").unwrap();
        assert_eq!(ra.env_for(e, "GB", false), ra.env(false));
        assert_eq!(ra.env_for(e, "GB", true), ra.env(true));
        assert!(
            ra.env_for(e, "GBA", false).is_empty(),
            "not its achievements' system"
        );
        let casual = described(&format!("{both}[achievements]\nsystems = [\"GB\"]\n"));
        let e = casual.get("e").unwrap();
        assert_eq!(
            ra.env_for(e, "GB", true)[2].1,
            "0",
            "Casual only: no hardcore"
        );
        let none = described(both);
        assert!(none.get("e").unwrap().achievements.systems.is_empty());
        assert!(ra.env_for(none.get("e").unwrap(), "GB", false).is_empty());
    }

    #[test]
    fn which_consoles_are_hashed() {
        assert!(hashed(Console::Snes) && hashed(Console::PlayStation));
        assert!(!hashed(Console::GameWatch) && !hashed(Console::Ports));
    }

    #[test]
    fn a_sign_in_is_kept_before_anyone_hears_of_it() {
        let dir = TempDir::new("ra-sign-in");
        let path = dir.path().join("retroachievements.token");
        let got = keep_sign_in(&path, Ok((standing("Alan"), "t0k3n".into())));
        let account = Account {
            user: "Alan".into(),
            token: "t0k3n".into(),
        };
        assert_eq!(got, Ok((account.clone(), standing("Alan"))));
        assert_eq!(Account::load(&path), Some(account));
        // Refused: nothing written, the service's words passed on.
        let other = dir.path().join("other.token");
        let refused = keep_sign_in(&other, Err(Error::Refused("Invalid".into())));
        assert_eq!(refused, Err(Error::Refused("Invalid".into())));
        assert!(!other.exists());
    }

    #[test]
    fn a_refused_check_drops_the_token_unless_it_changed_since() {
        let dir = TempDir::new("ra-check");
        let path = dir.path().join("retroachievements.token");
        let alan = Account {
            user: "Alan".into(),
            token: "old".into(),
        };
        alan.save(&path).unwrap();
        // Still good, with a new token: the new one is kept.
        let got = checked(&path, &alan, Ok((standing("Alan"), "new".into())));
        let renewed = Account {
            user: "Alan".into(),
            token: "new".into(),
        };
        assert_eq!(got.unwrap().0, renewed);
        assert_eq!(Account::load(&path), Some(renewed.clone()));
        // Refused, but someone else signed in meanwhile: theirs stands.
        let refused = || Err(Error::Refused("Expired".into()));
        assert!(checked(&path, &alan, refused()).is_err());
        assert_eq!(Account::load(&path), Some(renewed.clone()));
        // Refused, the file as it was checked: dropped.
        assert!(checked(&path, &renewed, refused()).is_err());
        assert!(!path.exists());
        // No network: the sign-in kept stands as it is.
        renewed.save(&path).unwrap();
        assert_eq!(
            checked(&path, &renewed, Err(Error::NoNetwork)),
            Err(Error::NoNetwork)
        );
        assert!(path.exists());
    }

    #[test]
    fn the_sign_up_page_makes_a_code() {
        let code = qr(SIGN_UP).unwrap();
        // Version 3 or 4 at medium correction for this length.
        assert!((29..=37).contains(&code.size), "{}", code.size);
        assert_eq!(code.modules.len(), code.size * code.size);
        // The finder pattern's corner.
        assert!(code.dark(0, 0) && code.dark(6, 6) && !code.dark(7, 7));
    }
}
