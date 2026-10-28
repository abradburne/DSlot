//! The service's requests, through nibkit's `nib-cheevos` (`api`): signing in, a game's sets and
//! the player's unlocks in them, and a badge. rcheevos builds every request and reads every
//! answer (`rc_api_*`), so they are spelt and checked as an emulator's session does; what is
//! here is what DSlot makes of the answers: the player's [`Standing`], a game's [`Set`] as the
//! shelf shows it, and an [`Error`] worded for a row.
//!
//! Every request goes by `curl`, the form on its standard input, never in an argument: the
//! password and the token are in it. A refusal of the sign-in is told apart from the rest
//! whichever request it answers (`invalid_credentials`, `expired_token`, `access_denied`, or an
//! HTTP 401 or 403, as rcheevos reads them).

use super::set::{Achievement, Set};
use super::{Account, Standing};
use nib_cheevos::api::{self, Secret, Service};
use nib_cheevos::launch::Credentials;
use std::path::Path;
use std::sync::OnceLock;

pub use nib_cheevos::paths::WARNING_ID;

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

impl From<api::Error> for Error {
    fn from(e: api::Error) -> Error {
        match e {
            api::Error::Refused(why) => Error::Refused(why),
            api::Error::NoNetwork => Error::NoNetwork,
            api::Error::Failed(why) => Error::Failed(why),
        }
    }
}

/// The service, as DSlot: its User-Agent names DSlot, its version and the system, then
/// rcheevos (`DSlot/0.4.1 (nibOS; linux aarch64) rcheevos/12.4`).
fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(|| {
        Service::new(nib_cheevos::ua::user_agent(
            "DSlot",
            env!("CARGO_PKG_VERSION"),
            None,
        ))
    })
}

/// Signs in with a password (`Secret::Password`) or the token kept: the player's standing, and
/// the token to keep.
pub fn login(user: &str, secret: Secret<'_>) -> Result<(Standing, String), Error> {
    let login = service().login(user, secret)?;
    Ok((
        Standing {
            user: login.username,
            score: login.score.into(),
            softcore: login.softcore.into(),
            messages: login.messages.into(),
        },
        login.token,
    ))
}

/// The game a hash is and the player's unlocks in it, softcore and hardcore, as the shelf shows
/// it. Ok(None): the service doesn't know the ROM.
pub fn set(account: &Account, hash: &str) -> Result<Option<Set>, Error> {
    let credentials = Credentials {
        username: account.user.clone(),
        token: account.token.clone(),
    };
    Ok(service()
        .game_and_unlocks(&credentials, hash)?
        .map(|(game, unlocked)| set_of(game, &unlocked)))
}

/// A game's sets as the shelf shows them: their published achievements (never the unofficial
/// ones, nor the service's warnings), each marked earned when `unlocked` has it.
fn set_of(game: api::Game, unlocked: &[u32]) -> Set {
    let achievements = game
        .achievements
        .into_iter()
        .filter(|a| a.official && a.id < WARNING_ID)
        .map(|a| Achievement {
            unlocked: unlocked.contains(&a.id),
            id: a.id,
            title: a.title,
            description: a.description,
            points: a.points,
            badge: a.badge_name,
            unlocked_at: None,
        })
        .collect();
    Set {
        id: game.id,
        title: game.title,
        achievements,
    }
}

/// A badge (`<name>.png` on the media host) fetched into `to`, whole or not at all.
pub fn badge(name: &str, to: &Path) -> Result<(), Error> {
    Ok(service().badge(name, to)?)
}

/// Whether there is a `curl` to make requests with.
pub fn available() -> bool {
    service().available()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(id: u32, official: bool) -> api::Achievement {
        api::Achievement {
            id,
            title: format!("A{id}"),
            points: id,
            badge_name: format!("{id:05}"),
            official,
            ..api::Achievement::default()
        }
    }

    #[test]
    fn a_set_with_its_published_achievements_and_the_players_unlocks() {
        let game = api::Game {
            id: 12345,
            title: "Advance Wars: Days of Ruin".into(),
            achievements: vec![
                one(1, true),
                one(2, false),
                one(3, true),
                one(WARNING_ID, true),
            ],
            ..api::Game::default()
        };
        let set = set_of(game, &[3, 99]);
        assert_eq!(
            (set.id, set.title.as_str()),
            (12345, "Advance Wars: Days of Ruin")
        );
        let ids: Vec<_> = set.achievements.iter().map(|a| a.id).collect();
        assert_eq!(ids, [1, 3]);
        assert_eq!(set.achievements[1].badge, "00003");
        assert_eq!(set.unlocked(), 1);
        assert_eq!(set.points(), (3, 4));
    }

    #[test]
    fn the_service_errors_are_dslots() {
        assert_eq!(
            Error::from(api::Error::Refused("Expired".into())),
            Error::Refused("Expired".into())
        );
        assert_eq!(Error::from(api::Error::NoNetwork), Error::NoNetwork);
        assert_eq!(Error::Failed("x".into()).text(), "Didn't work: x");
    }
}
