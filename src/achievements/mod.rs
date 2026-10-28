//! RetroAchievements: the player's sign-in, kept for the emulators to use.
//!
//! DSlot signs in once, with the username and password typed on its keyboard, through the
//! service's own `dorequest.php` (`r=login2`, the request rcheevos makes), and keeps what the
//! service answers with: the username as the service spells it and a **token**, which signs the
//! player in from then on without the password. The password is sent once and never written
//! anywhere. The token is kept in a file of its own (`retroachievements.token`, beside
//! settings.toml, readable by DSlot's user only), two lines, `username` then `token`: the
//! format DSperate's `--cheevos-token` reads. An emulator gets it through its environment when
//! a game starts ([`env`]): `RA_TOKEN_FILE`, `RA_USERNAME` and `RA_HARDCORE`; never the
//! password, never a word of it on the command line.
//!
//! The requests go through `curl`, on a thread, as nibOS's own tools do (`nib update`,
//! `nib bug-report`): DSlot has no TLS of its own. Where there is no `curl` the page isn't
//! offered ([`available`]).
//!
//! The service has no way to make an account from a device: its public API reads data only,
//! and rcheevos' endpoints sign in, start sessions and award. So the page shows the sign-up
//! page's address and a QR code of it, for a phone ([`SIGN_UP`], [`qr`]).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

/// The service, and the request its clients make.
pub mod hash;
pub mod set;

pub const HOST: &str = "https://retroachievements.org";
/// Where an account is made, on a phone or a computer.
pub const SIGN_UP: &str = "https://retroachievements.org/createaccount.php";
/// How long a sign-in waits for the service.
const TIMEOUT_SECS: u32 = 20;

/// The environment an emulator reads the sign-in from (docs/emulators.md).
pub const ENV_TOKEN_FILE: &str = "RA_TOKEN_FILE";
pub const ENV_USERNAME: &str = "RA_USERNAME";
pub const ENV_HARDCORE: &str = "RA_HARDCORE";

/// Whether the page is offered: `curl` is there to talk to the service.
pub fn available() -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain([std::path::PathBuf::from("/usr/bin")])
        .any(|dir| dir.join("curl").is_file())
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
        let text = std::fs::read_to_string(path).ok()?;
        let mut lines = text.lines().map(str::trim);
        let user = lines.next().filter(|s| !s.is_empty())?.to_owned();
        let token = lines.next().filter(|s| !s.is_empty())?.to_owned();
        Some(Account { user, token })
    }

    /// Writes the token file, readable by DSlot's user only.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        #[cfg(unix)]
        {
            // A file from before keeps its mode through truncation: set it again.
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        write!(file, "{}\n{}\n", self.user, self.token)?;
        file.sync_all()
    }

    /// Removes the token file: signed out.
    pub fn clear(path: &Path) -> std::io::Result<()> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
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
    /// "1,234 points", "softcore 56", both, or "No points yet".
    pub fn points(&self) -> String {
        let n = |v: u64| {
            let s = v.to_string();
            let mut out = String::new();
            for (i, c) in s.chars().enumerate() {
                if i > 0 && (s.len() - i).is_multiple_of(3) {
                    out.push(',');
                }
                out.push(c);
            }
            out
        };
        match (self.score, self.softcore) {
            (0, 0) => "No points yet".into(),
            (h, 0) => format!("{} points", n(h)),
            (0, s) => format!("{} softcore points", n(s)),
            (h, s) => format!("{} points \u{b7} {} softcore", n(h), n(s)),
        }
    }
}

/// How a request came out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Signed in: the player's standing and the token to keep.
    SignedIn(Standing, String),
    /// The service said no: wrong password, an unknown user, a token no longer good.
    Refused(String),
    /// The service couldn't be reached: no network, or it didn't answer in time.
    NoNetwork,
    /// Something else went wrong (an odd answer, `curl` missing).
    Failed(String),
}

impl Outcome {
    /// A line for the row or the title.
    pub fn text(&self) -> String {
        match self {
            Outcome::SignedIn(s, _) => format!("Signed in as {}", s.user),
            Outcome::Refused(why) => why.clone(),
            Outcome::NoNetwork => "No network: turn Wi-Fi on and try again".into(),
            Outcome::Failed(why) => format!("Didn't work: {why}"),
        }
    }
}

/// Signs in with a password: the service answers with the token to keep. The password
/// goes to `curl` on its standard input, never in an argument.
pub fn sign_in(user: String, password: String) -> Receiver<Outcome> {
    let form = format!("r=login2&u={}&p={}", urlencode(&user), urlencode(&password));
    request(form)
}

/// Signs in again with the token kept: whether it is still good, and the standing now.
pub fn check(account: &Account) -> Receiver<Outcome> {
    let form = format!(
        "r=login2&u={}&t={}",
        urlencode(&account.user),
        urlencode(&account.token)
    );
    request(form)
}

/// Posts `form` to the service on a thread; the outcome arrives on the receiver.
fn request(form: String) -> Receiver<Outcome> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(post(&form));
    });
    rx
}

/// `curl`, the form on its standard input, the answer parsed.
fn post(form: &str) -> Outcome {
    match call(form) {
        Ok(body) => parse(Some(0), &body, ""),
        Err(Fail::NoNetwork) => Outcome::NoNetwork,
        Err(Fail::Failed(why)) => Outcome::Failed(why),
    }
}

/// Why a request got no answer: the service couldn't be reached, or something else, said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fail {
    NoNetwork,
    Failed(String),
}

/// Posts `form` to the service's emulator API by `curl`, the form on its standard input
/// (never in an argument: the token is in it), and gives back the body of the answer.
pub(crate) fn call(form: &str) -> Result<String, Fail> {
    let mut child = curl()
        .args([
            "-H",
            "Content-Type: application/x-www-form-urlencoded",
            "--data-binary",
            "@-",
            &format!("{HOST}/dorequest.php"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Fail::Failed(format!("curl: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(form.as_bytes());
    }
    let out = child
        .wait_with_output()
        .map_err(|e| Fail::Failed(format!("curl: {e}")))?;
    exit(out.status.code(), &String::from_utf8_lossy(&out.stderr))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Fetches `url` into `to` by `curl`: written beside it first, so a half-fetched file is
/// never where a whole one is looked for. A 404 (a badge the service hasn't got) fails.
pub(crate) fn fetch(url: &str, to: &Path) -> Result<(), Fail> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Fail::Failed(e.to_string()))?;
    }
    let part = to.with_extension("part");
    let out = curl()
        .args(["-f", "-o"])
        .arg(&part)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Fail::Failed(format!("curl: {e}")))?;
    let done = exit(out.status.code(), &String::from_utf8_lossy(&out.stderr));
    if done.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    done?;
    std::fs::rename(&part, to).map_err(|e| Fail::Failed(e.to_string()))
}

/// `curl` as DSlot runs it: quiet but for errors, a time limit, DSlot's user agent.
fn curl() -> Command {
    let mut c = Command::new("curl");
    c.args([
        "-sS",
        "--max-time",
        &TIMEOUT_SECS.to_string(),
        "-A",
        &format!("DSlot/{}", env!("CARGO_PKG_VERSION")),
    ]);
    c
}

/// `curl`'s exit code as a failure: couldn't resolve, couldn't connect, timed out and SSL
/// couldn't connect are no network; anything else but 0 is what it said.
fn exit(code: Option<i32>, stderr: &str) -> Result<(), Fail> {
    match code {
        Some(0) => Ok(()),
        Some(6 | 7 | 28 | 35) => Err(Fail::NoNetwork),
        Some(c) => {
            let why = stderr.lines().next().unwrap_or("").trim();
            Err(Fail::Failed(if why.is_empty() {
                format!("curl exited with {c}")
            } else {
                why.to_owned()
            }))
        }
        None => Err(Fail::Failed("curl was killed".into())),
    }
}

/// `curl`'s exit code and output, as an outcome. Nothing of the request is in it.
pub fn parse(code: Option<i32>, body: &str, stderr: &str) -> Outcome {
    match exit(code, stderr) {
        Ok(()) => {}
        Err(Fail::NoNetwork) => return Outcome::NoNetwork,
        Err(Fail::Failed(why)) => return Outcome::Failed(why),
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return Outcome::Failed("the service answered with something other than JSON".into());
    };
    let text = |key: &str| json.get(key).and_then(|v| v.as_str()).map(str::to_owned);
    let num = |key: &str| json.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
    if json.get("Success").and_then(|v| v.as_bool()) != Some(true) {
        let why = text("Error").unwrap_or_else(|| "The service said no".into());
        return Outcome::Refused(plain(&why));
    }
    match (text("User"), text("Token")) {
        (Some(user), Some(token)) if !user.is_empty() && !token.is_empty() => Outcome::SignedIn(
            Standing {
                user,
                score: num("Score"),
                softcore: num("SoftcoreScore"),
                messages: num("Messages"),
            },
            token,
        ),
        _ => Outcome::Failed("the service's answer had no token in it".into()),
    }
}

/// The service's error messages, without their HTML escapes and trailing exhortations.
fn plain(why: &str) -> String {
    let why = why.replace("\\/", "/");
    let why = why.trim_end_matches(" Please try again.").trim();
    why.to_owned()
}

/// application/x-www-form-urlencoded: letters, digits and `-_.~` as they are, the rest as
/// `%XX` of their UTF-8 bytes.
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
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

/// The environment a game starts with when the player is signed in: where the token file
/// is, who it signs in as, and whether they chose hardcore (`1`) or softcore (`0`).
pub fn env(token: &Path, hardcore: bool) -> Vec<(String, String)> {
    let Some(account) = Account::load(token) else {
        return Vec::new();
    };
    vec![
        (ENV_TOKEN_FILE.into(), token.to_string_lossy().into_owned()),
        (ENV_USERNAME.into(), account.user),
        (ENV_HARDCORE.into(), if hardcore { "1" } else { "0" }.into()),
    ]
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

    #[test]
    fn a_sign_in_is_parsed() {
        let body = r#"{"Success":true,"User":"Alan","Token":"abc123","Score":1234,"SoftcoreScore":5,"Messages":0,"AvatarUrl":"x"}"#;
        assert_eq!(
            parse(Some(0), body, ""),
            Outcome::SignedIn(
                Standing {
                    user: "Alan".into(),
                    score: 1234,
                    softcore: 5,
                    messages: 0,
                },
                "abc123".into()
            )
        );
    }

    #[test]
    fn a_refusal_and_the_rest() {
        let body = r#"{"Success":false,"Status":401,"Code":"invalid_credentials","Error":"Invalid user\/password combination. Please try again."}"#;
        assert_eq!(
            parse(Some(0), body, ""),
            Outcome::Refused("Invalid user/password combination.".into())
        );
        assert_eq!(
            parse(Some(6), "", "curl: (6) Could not resolve host"),
            Outcome::NoNetwork
        );
        assert_eq!(parse(Some(28), "", ""), Outcome::NoNetwork);
        assert!(matches!(parse(Some(0), "<html>", ""), Outcome::Failed(_)));
        assert!(matches!(
            parse(Some(0), r#"{"Success":true,"User":"x"}"#, ""),
            Outcome::Failed(_)
        ));
        assert_eq!(
            parse(
                Some(22),
                "",
                "curl: (22) The requested URL returned error: 500"
            ),
            Outcome::Failed("curl: (22) The requested URL returned error: 500".into())
        );
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
        assert_eq!(s(0, 56).points(), "56 softcore points");
        assert_eq!(s(1000, 999).points(), "1,000 points \u{b7} 999 softcore");
    }

    #[test]
    fn the_form_is_encoded() {
        assert_eq!(urlencode("Al-an_.~"), "Al-an_.~");
        assert_eq!(urlencode("p&ss w=rd/é"), "p%26ss%20w%3Drd%2F%C3%A9");
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
        let vars = env(&path, true);
        assert_eq!(vars[0].0, "RA_TOKEN_FILE");
        assert_eq!(vars[1], ("RA_USERNAME".to_owned(), "Alan".to_owned()));
        assert_eq!(vars[2], ("RA_HARDCORE".to_owned(), "1".to_owned()));
        assert_eq!(vars.len(), 3);
        // A half file is no sign-in.
        std::fs::write(&path, "Alan\n").unwrap();
        assert_eq!(Account::load(&path), None);
        assert!(env(&path, false).is_empty());
        Account::clear(&path).unwrap();
        Account::clear(&path).unwrap();
        assert!(!path.exists());
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
