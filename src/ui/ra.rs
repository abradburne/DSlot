//! The menu's **Achievements** page: the RetroAchievements sign-in (achievements/). Signed out,
//! the username typed on the keyboard, Sign in (which asks for the password) and Make an
//! account (the sign-up page's address and a QR code of it, for a phone); signed in, who and
//! their points, and Sign out. The Mode row (hardcore or softcore) only while hardcore is
//! offered; until then the page ends with a note that achievements are Casual for now.
//!
//! A [`RaPage`] holds what the page knows: the sign-in kept, the username typed so far, what
//! the service said of the player, and the one request running (a sign-in, or the check of the
//! sign-in kept made once each time the menu opens on the page). The requests keep or drop the
//! token themselves, on their own threads (achievements::Ra); the page only hears how they came
//! out, and says when the player signed in or out so that the shelf lets go of what it found
//! for the last one. The menu keeps the rows' places, the keyboard and the notes.

use super::menu::{Item, Value, NOTES_WIDTH};
use crate::achievements::{self, Account, Error, Qr, Ra, SignedIn, Standing};
use crate::settings::def;
use crate::update;
use std::cell::OnceCell;
use std::sync::mpsc::Receiver;

/// What the request running is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    /// Signing in with a password.
    SignIn,
    /// Checking the sign-in kept.
    Check,
}

/// What a request came to, for the menu: a line to show under the title (none when it went
/// well), and whether the player signed in or out by it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    pub line: Option<String>,
    pub changed: bool,
}

pub struct RaPage {
    ra: Ra,
    account: Option<Account>,
    /// The username typed so far: the one signed in as, to start with.
    user: String,
    standing: Option<Standing>,
    job: Option<(Job, Receiver<SignedIn>)>,
    /// The last outcome's line, for the title.
    said: Option<String>,
    /// The sign-in kept has been checked with the service this time the menu is open.
    checked: bool,
    /// The sign-up page's address and QR code are open, in place of the rows.
    signing_up: bool,
    /// The sign-up page's QR code, made the first time it is shown.
    qr: OnceCell<Option<Qr>>,
}

impl RaPage {
    /// The page as the menu opens: the sign-in kept, read once.
    pub fn new(ra: &Ra) -> RaPage {
        let account = ra.account();
        RaPage {
            ra: ra.clone(),
            user: account.as_ref().map(|a| a.user.clone()).unwrap_or_default(),
            account,
            standing: None,
            job: None,
            said: None,
            checked: false,
            signing_up: false,
            qr: OnceCell::new(),
        }
    }

    pub fn signed_in(&self) -> bool {
        self.account.is_some()
    }

    /// A request is running.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn set_user(&mut self, user: &str) {
        self.user = user.trim().to_owned();
    }

    /// The sign-up page is open in place of the rows.
    pub fn signing_up(&self) -> bool {
        self.signing_up
    }

    pub fn sign_up(&mut self, open: bool) {
        self.signing_up = open;
    }

    /// The rows: signed out, the username, Sign in, Mode and Make an account; signed in, who,
    /// Sign out and Mode. Mode only while hardcore is offered (achievements::HARDCORE_OFFERED);
    /// until then the page ends with a note that achievements are Casual for now. The sign-up
    /// view has the address alone, over the QR code.
    pub fn rows(&self) -> Vec<Item> {
        if self.signing_up {
            return vec![Item::RaLink];
        }
        let mode = def::builtin(def::ACHIEVEMENTS)
            .filter(|_| achievements::HARDCORE_OFFERED)
            .map(Item::Setting);
        let mut items = Vec::new();
        match &self.account {
            Some(_) => {
                items.extend([Item::RaAccount, Item::RaSignOut]);
                items.extend(mode);
            }
            None => {
                items.extend([Item::RaUser, Item::RaSignIn]);
                items.extend(mode);
                items.push(Item::RaSignUp);
            }
        }
        if !achievements::HARDCORE_OFFERED {
            items.extend(
                update::wrap(achievements::CASUAL_ONLY, NOTES_WIDTH)
                    .into_iter()
                    .map(Item::Message),
            );
        }
        items
    }

    /// The title line: who is signed in, or what the service said.
    pub fn title(&self) -> String {
        if self.signing_up {
            return "Make an account on a phone: scan the code".into();
        }
        match (&self.said, &self.account, &self.job) {
            (Some(said), ..) => said.clone(),
            (None, _, Some((Job::SignIn, _))) => "Signing in to RetroAchievements\u{2026}".into(),
            (None, Some(a), _) => format!("RetroAchievements: signed in as {}", a.user),
            (None, None, _) => "RetroAchievements: not signed in".into(),
        }
    }

    /// A row's value: `here`, the row is highlighted; `armed`, A has been pressed on it once.
    pub fn value(&self, item: &Item, here: bool, armed: bool) -> Value {
        let prompt = |text: String, warning: bool| Value::Prompt { text, warning };
        let press = |what: &str| prompt(if here { what.into() } else { String::new() }, false);
        match item {
            Item::RaUser if self.user.is_empty() => press("Press A to type it"),
            Item::RaUser => Value::Text(self.user.clone()),
            Item::RaSignIn if self.busy() => prompt("Signing in\u{2026}".into(), false),
            Item::RaSignIn => press("Press A for the password"),
            Item::RaAccount => {
                let user = self
                    .account
                    .as_ref()
                    .map_or("\u{2013}", |a| a.user.as_str());
                match &self.standing {
                    Some(s) => Value::Status {
                        text: format!("{user} \u{b7} {}", s.points()),
                        glyphs: vec![nibkit::menu::Glyph::Check],
                    },
                    None if self.busy() => prompt(format!("{user} \u{b7} checking\u{2026}"), false),
                    None => prompt(user.to_owned(), false),
                }
            }
            Item::RaSignOut if armed => prompt("Press A again to sign out".into(), true),
            Item::RaSignOut | Item::RaSignUp => press("Press A"),
            Item::RaLink => prompt(
                achievements::SIGN_UP
                    .trim_start_matches("https://")
                    .to_owned(),
                false,
            ),
            _ => prompt(String::new(), false),
        }
    }

    /// The sign-up page's QR code, while it is open.
    pub fn picture(&self) -> Option<Qr> {
        if !self.signing_up {
            return None;
        }
        self.qr
            .get_or_init(|| achievements::qr(achievements::SIGN_UP))
            .clone()
    }

    /// Signs in as the username typed, with `password`. One request at a time.
    pub fn sign_in(&mut self, password: &str) {
        if self.busy() || self.user.is_empty() {
            return;
        }
        self.said = None;
        self.job = Some((Job::SignIn, self.ra.sign_in(&self.user, password)));
    }

    /// Signs out: the token file is removed. The username stays typed for next time.
    pub fn sign_out(&mut self) {
        self.ra.sign_out();
        (self.account, self.standing, self.said) = (None, None, None);
    }

    /// Checks the sign-in kept with the service, once per opening of the menu: whether the
    /// token still stands, and the player's points now.
    pub fn check(&mut self) {
        if self.checked || self.busy() {
            return;
        }
        self.checked = true;
        if let Some(account) = &self.account {
            self.job = Some((Job::Check, self.ra.check(account.clone())));
        }
    }

    /// The request's outcome, once it has come: the sign-in kept, a refusal or a failure,
    /// said. The token was kept or dropped already, by the request.
    pub fn poll(&mut self) -> Option<Heard> {
        let (job, rx) = self.job.as_ref()?;
        let job = *job;
        let outcome = rx.try_recv().ok()?;
        self.job = None;
        let was = self.account.clone();
        let line = match (job, outcome) {
            (Job::SignIn, Ok((account, standing))) => {
                self.user = account.user.clone();
                (self.account, self.standing) = (Some(account), Some(standing));
                None
            }
            // Signed out while it was checked: nothing to bring back.
            (Job::Check, Ok(_)) if self.account.is_none() => None,
            (Job::Check, Ok((account, standing))) => {
                (self.account, self.standing) = (Some(account), Some(standing));
                None
            }
            // The token no longer stands (the password changed, say): sign in again.
            (Job::Check, Err(Error::Refused(_))) => {
                (self.account, self.standing) = (None, None);
                Some("The sign-in kept is no longer good: sign in again".to_owned())
            }
            // Offline: the sign-in kept stands as it is.
            (Job::Check, Err(Error::NoNetwork)) => None,
            (_, Err(e)) => {
                eprintln!("dslot: RetroAchievements: {}", e.text());
                Some(e.text())
            }
        };
        self.said = line.clone();
        let changed = was.as_ref().map(|a| &a.user) != self.account.as_ref().map(|a| &a.user);
        Some(Heard { line, changed })
    }

    /// Back as the menu leaves the page: the sign-up view closed.
    pub fn leave(&mut self) {
        self.signing_up = false;
    }

    /// A request running that doesn't answer until the test lets it.
    #[cfg(test)]
    pub fn waiting_on(&mut self, sign_in: bool) -> std::sync::mpsc::Sender<SignedIn> {
        let (tx, rx) = std::sync::mpsc::channel();
        let job = if sign_in { Job::SignIn } else { Job::Check };
        self.job = Some((job, rx));
        tx
    }
}
