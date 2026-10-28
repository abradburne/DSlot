//! The service's emulator API (`dorequest.php`, the requests rcheevos makes) and its media host,
//! by `curl`: signing in, a game's set, the player's unlocks in it, and a badge. Everything that
//! knows how a request is spelt or an answer is shaped is here, so that it can give way to
//! rcheevos' own `rc_api_*` requests in one piece.
//!
//! Every request is a form posted on `curl`'s standard input, never in an argument: the
//! password and the token are in it. Every answer is checked the same way ([`reply`]):
//! `Success`, and when it is false the service's `Error` line, sorted by its `Code` into a
//! refusal of the sign-in or some other failure ([`Error`]).

use super::set::{Achievement, Set};
use super::{Account, Standing};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const HOST: &str = "https://retroachievements.org";
/// The media host the badges come from.
pub const MEDIA: &str = "https://media.retroachievements.org";
/// How long a request waits for the service.
const TIMEOUT_SECS: u32 = 20;
/// `Flags` of an achievement in the set proper; others are unofficial, not shown.
const CORE: u64 = 3;
/// The first id of the service's own "Warning: Unknown Emulator" entries ("Hardcore unlocks
/// cannot be earned using this emulator"), which it puts in the set of an emulator it doesn't
/// know, earned already. Not achievements: never shown or counted, as DSperate and inkdot
/// leave them out too, while hardcore isn't offered (achievements::HARDCORE_OFFERED).
pub const WARNING_ID: u32 = 101_000_001;

/// Why a request came to nothing, for signing in and for the shelf's lookups alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The service refused the sign-in: a wrong password, an unknown user, a token no longer
    /// good. The service's own words.
    Refused(String),
    /// The service couldn't be reached: no network, or it didn't answer in time.
    NoNetwork,
    /// Something else went wrong (an odd answer, the service failing, `curl` missing).
    Failed(String),
}

impl Error {
    /// A line for a row or a title.
    pub fn text(&self) -> String {
        match self {
            Error::Refused(why) => why.clone(),
            Error::NoNetwork => "No network: turn Wi-Fi on and try again".into(),
            Error::Failed(why) => format!("Didn't work: {why}"),
        }
    }
}

/// Signs in with a password or the token kept (`login2`): the player's standing, and the
/// token to keep.
pub fn login(form: &str) -> Result<(Standing, String), Error> {
    parse_login(&call(&format!("r=login2&{form}"))?)
}

/// The `login2` answer: the player's standing, and the token.
pub fn parse_login(body: &str) -> Result<(Standing, String), Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Login {
        user: Option<String>,
        token: Option<String>,
        #[serde(default)]
        score: Option<u64>,
        #[serde(default)]
        softcore_score: Option<u64>,
        #[serde(default)]
        messages: Option<u64>,
    }
    let login: Login = reply(body)?;
    match (login.user, login.token) {
        (Some(user), Some(token)) if !user.is_empty() && !token.is_empty() => Ok((
            Standing {
                user,
                score: login.score.unwrap_or(0),
                softcore: login.softcore_score.unwrap_or(0),
                messages: login.messages.unwrap_or(0),
            },
            token,
        )),
        _ => Err(Error::Failed(
            "the service's answer had no token in it".into(),
        )),
    }
}

/// The form that signs in with a password.
pub fn password_form(user: &str, password: &str) -> String {
    format!("u={}&p={}", urlencode(user), urlencode(password))
}

/// The form that signs in with the token kept: every request but the first sign-in.
pub fn token_form(account: &Account) -> String {
    format!(
        "u={}&t={}",
        urlencode(&account.user),
        urlencode(&account.token)
    )
}

/// The set for a hash and the player's unlocks in it (`patch`, then `unlocks` softcore and
/// hardcore). Ok(None): the service doesn't know the ROM.
pub fn set(account: &Account, hash: &str) -> Result<Option<Set>, Error> {
    let auth = token_form(account);
    let body = call(&format!("r=patch&{auth}&m={}", urlencode(hash)))?;
    let Some(mut set) = parse_patch(&body)? else {
        return Ok(None);
    };
    let mut unlocked = Vec::new();
    for hardcore in [0, 1] {
        let body = call(&format!("r=unlocks&{auth}&g={}&h={hardcore}", set.id))?;
        unlocked.extend(parse_unlocks(&body)?);
    }
    for a in &mut set.achievements {
        a.unlocked = unlocked.contains(&a.id);
    }
    Ok(Some(set))
}

/// The `patch` answer: the set, or None when the service says it doesn't know the game.
pub fn parse_patch(body: &str) -> Result<Option<Set>, Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Patch {
        patch_data: Option<Data>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Data {
        #[serde(rename = "ID", default)]
        id: Option<u32>,
        title: Option<String>,
        #[serde(default)]
        achievements: Option<Vec<One>>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct One {
        #[serde(rename = "ID")]
        id: u32,
        title: Option<String>,
        description: Option<String>,
        points: Option<u32>,
        flags: Option<u64>,
        badge_name: Option<String>,
    }
    let patch: Patch = match reply(body) {
        Ok(patch) => patch,
        // An unknown hash is answered with an error naming it, not a set: that's an answer.
        Err(Error::Failed(why) | Error::Refused(why)) if unknown_game(&why) => return Ok(None),
        Err(e) => return Err(e),
    };
    let Some(data) = patch.patch_data else {
        return Err(Error::Failed(
            "the service's answer had no set in it".into(),
        ));
    };
    let id = data.id.unwrap_or(0);
    if id == 0 {
        return Ok(None);
    }
    let text = |s: Option<String>| s.unwrap_or_default().trim().to_owned();
    let achievements = data
        .achievements
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.flags == Some(CORE) && a.id < WARNING_ID)
        .map(|a| Achievement {
            id: a.id,
            title: text(a.title),
            description: text(a.description),
            points: a.points.unwrap_or(0),
            badge: text(a.badge_name),
            unlocked: false,
            unlocked_at: None,
        })
        .collect();
    Ok(Some(Set {
        id,
        title: data.title.unwrap_or_default(),
        achievements,
    }))
}

/// The service's ways of saying it has no set for a hash.
fn unknown_game(why: &str) -> bool {
    let why = why.to_ascii_lowercase();
    why.contains("unknown game") || why.contains("not found") || why.contains("no game")
}

/// The `unlocks` answer: the ids the player has.
pub fn parse_unlocks(body: &str) -> Result<Vec<u32>, Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Unlocks {
        #[serde(default)]
        user_unlocks: Option<Vec<u32>>,
    }
    Ok(reply::<Unlocks>(body)?.user_unlocks.unwrap_or_default())
}

/// A badge (`<name>.png` on the media host) fetched into `to`.
pub fn badge(name: &str, to: &Path) -> Result<(), Error> {
    fetch(&format!("{MEDIA}/Badge/{name}.png"), to)
}

/// What every answer of the emulator API says first: whether it did what was asked, and if
/// not, why, in words and as a code.
#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct Status {
    success: Option<bool>,
    error: Option<String>,
    code: Option<String>,
    status: Option<u16>,
}

/// The codes, and the HTTP statuses, the service refuses a sign-in with: rcheevos'
/// `RC_INVALID_CREDENTIALS`, `RC_EXPIRED_TOKEN` and `RC_ACCESS_DENIED`.
fn refusal(status: &Status) -> bool {
    matches!(
        status.code.as_deref(),
        Some("invalid_credentials" | "expired_token" | "access_denied")
    ) || matches!(status.status, Some(401 | 403))
}

/// An answer of the emulator API, checked: `Success` true, then the rest read as `T`. A
/// refusal of the sign-in is [`Error::Refused`], any other `Success: false`
/// [`Error::Failed`], both with the service's own words.
pub fn reply<T: DeserializeOwned>(body: &str) -> Result<T, Error> {
    let json: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| Error::Failed("the service answered with something other than JSON".into()))?;
    let status = Status::deserialize(&json).unwrap_or_default();
    if status.success != Some(true) {
        let why = plain(status.error.as_deref().unwrap_or("The service said no"));
        return Err(if refusal(&status) {
            Error::Refused(why)
        } else {
            Error::Failed(why)
        });
    }
    serde_json::from_value(json)
        .map_err(|e| Error::Failed(format!("the service's answer was odd: {e}")))
}

/// The service's error messages, without their HTML escapes and trailing exhortations.
fn plain(why: &str) -> String {
    let why = why.replace("\\/", "/");
    let why = why.trim_end_matches(" Please try again.").trim();
    why.to_owned()
}

/// Posts `form` to the emulator API by `curl`, the form on its standard input, and gives back
/// the body of the answer.
fn call(form: &str) -> Result<String, Error> {
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
        .map_err(|e| Error::Failed(format!("curl: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(form.as_bytes());
    }
    let out = child
        .wait_with_output()
        .map_err(|e| Error::Failed(format!("curl: {e}")))?;
    exit(out.status.code(), &String::from_utf8_lossy(&out.stderr))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Fetches `url` into `to` by `curl`: written beside it first, so a half-fetched file is
/// never where a whole one is looked for. A 404 (a badge the service hasn't got) fails.
fn fetch(url: &str, to: &Path) -> Result<(), Error> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::Failed(e.to_string()))?;
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
        .map_err(|e| Error::Failed(format!("curl: {e}")))?;
    let done = exit(out.status.code(), &String::from_utf8_lossy(&out.stderr));
    if done.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    done?;
    std::fs::rename(&part, to).map_err(|e| Error::Failed(e.to_string()))
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
fn exit(code: Option<i32>, stderr: &str) -> Result<(), Error> {
    match code {
        Some(0) => Ok(()),
        Some(6 | 7 | 28 | 35) => Err(Error::NoNetwork),
        Some(c) => {
            let why = stderr.lines().next().unwrap_or("").trim();
            Err(Error::Failed(if why.is_empty() {
                format!("curl exited with {c}")
            } else {
                why.to_owned()
            }))
        }
        None => Err(Error::Failed("curl was killed".into())),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = r#"{"Success":true,"PatchData":{"ID":12345,"Title":"Advance Wars: Days of Ruin","ConsoleID":18,"ImageIcon":"/Images/000001.png","Achievements":[
      {"ID":1,"Title":"First Blood","Description":"Win a battle","Points":5,"Flags":3,"BadgeName":"00001","MemAddr":"0x1=1"},
      {"ID":2,"Title":"Test only","Description":"Unofficial","Points":1,"Flags":5,"BadgeName":"00002","MemAddr":"0x1=1"},
      {"ID":3,"Title":"Veteran","Description":null,"Points":25,"Flags":3,"BadgeName":"00003","MemAddr":"0x1=1"},
      {"ID":101000001,"Title":"Warning: Unknown Emulator","Description":"Hardcore unlocks cannot be earned using this emulator.","Points":0,"Flags":3,"BadgeName":"00000","MemAddr":"0=1"}
    ],"Leaderboards":[]}}"#;

    #[test]
    fn a_set_with_its_core_achievements_and_no_warning() {
        let set = parse_patch(PATCH).unwrap().unwrap();
        assert_eq!(
            (set.id, set.title.as_str()),
            (12345, "Advance Wars: Days of Ruin")
        );
        let ids: Vec<_> = set.achievements.iter().map(|a| a.id).collect();
        assert_eq!(ids, [1, 3]);
        assert_eq!(set.achievements[1].badge, "00003");
        assert_eq!(set.achievements[1].description, "", "a null is nothing");
        assert_eq!(set.points(), (0, 30));
        let ids = parse_unlocks(r#"{"Success":true,"UserUnlocks":[3,99],"GameID":12345}"#).unwrap();
        assert_eq!(ids, [3, 99]);
    }

    #[test]
    fn a_sign_in_is_parsed() {
        let body = r#"{"Success":true,"User":"Alan","Token":"abc123","Score":1234,"SoftcoreScore":5,"Messages":0,"AvatarUrl":"x"}"#;
        assert_eq!(
            parse_login(body),
            Ok((
                Standing {
                    user: "Alan".into(),
                    score: 1234,
                    softcore: 5,
                    messages: 0,
                },
                "abc123".into()
            ))
        );
        assert!(matches!(
            parse_login(r#"{"Success":true,"User":"x"}"#),
            Err(Error::Failed(_))
        ));
        assert!(matches!(parse_login("<html>"), Err(Error::Failed(_))));
    }

    #[test]
    fn a_hash_the_service_doesnt_know_is_an_answer() {
        assert_eq!(
            parse_patch(r#"{"Success":false,"Error":"Unknown game"}"#).unwrap(),
            None
        );
        assert_eq!(
            parse_patch(r#"{"Success":true,"PatchData":{"ID":0}}"#).unwrap(),
            None
        );
    }

    #[test]
    fn every_answer_is_checked_the_same_way() {
        // A revoked token on a lookup is the same refusal as a wrong password.
        let revoked = r#"{"Success":false,"Status":401,"Code":"invalid_credentials","Error":"Invalid user\/password combination. Please try again."}"#;
        let refused = Error::Refused("Invalid user/password combination.".into());
        assert_eq!(parse_patch(revoked), Err(refused.clone()));
        assert_eq!(parse_unlocks(revoked), Err(refused));
        assert_eq!(
            parse_unlocks(r#"{"Success":false,"Code":"expired_token","Error":"Expired"}"#),
            Err(Error::Refused("Expired".into()))
        );
        // The service failing otherwise is no refusal of the sign-in.
        assert_eq!(
            parse_patch(r#"{"Success":false,"Error":"Credentials invalid (0)"}"#),
            Err(Error::Failed("Credentials invalid (0)".into()))
        );
        assert!(matches!(parse_unlocks("not json"), Err(Error::Failed(_))));
        assert!(matches!(
            reply::<serde_json::Value>(r#"{"User":"x"}"#),
            Err(Error::Failed(_))
        ));
    }

    #[test]
    fn curl_failing_is_no_network_or_what_it_said() {
        assert_eq!(
            exit(Some(6), "curl: (6) Could not resolve host"),
            Err(Error::NoNetwork)
        );
        assert_eq!(exit(Some(28), ""), Err(Error::NoNetwork));
        assert_eq!(
            exit(Some(22), "curl: (22) The requested URL returned error: 500"),
            Err(Error::Failed(
                "curl: (22) The requested URL returned error: 500".into()
            ))
        );
        assert_eq!(exit(Some(0), ""), Ok(()));
    }

    #[test]
    fn the_forms_are_encoded() {
        assert_eq!(urlencode("Al-an_.~"), "Al-an_.~");
        assert_eq!(urlencode("p&ss w=rd/é"), "p%26ss%20w%3Drd%2F%C3%A9");
        assert_eq!(password_form("Al an", "p&w"), "u=Al%20an&p=p%26w");
        let account = Account {
            user: "Alan".into(),
            token: "t/k".into(),
        };
        assert_eq!(token_form(&account), "u=Alan&t=t%2Fk");
    }
}
