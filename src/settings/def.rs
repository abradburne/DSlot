//! DSlot's own settings: what each is, the values it takes, the levels it can be set at, its
//! default (nibkit's `Def`). An emulator's are declared in its description file and become
//! the same thing.

pub use nibkit::settings::{Def, Kind, Levels};
use toml::Value;

/// A setting with a few named values, kept as their keys.
macro_rules! choice {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $key:literal, $label:literal;)+ }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name { $($variant),+ }

        impl $name {
            const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn key(self) -> &'static str {
                match self { $($name::$variant => $key),+ }
            }

            pub fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            pub fn parse(key: &str) -> Option<$name> {
                Self::ALL.iter().copied().find(|v| v.key() == key)
            }

            fn choices() -> Kind {
                Kind::Choice(Self::ALL.iter().map(|v| (Value::from(v.key()), v.label().to_owned())).collect())
            }
        }
    };
}

choice! {
    /// The plastic a game's box comes in: clear in PAL regions, black everywhere else.
    Case {
        Region = "region", "By region";
        Clear = "clear", "Clear";
        Black = "black", "Black";
    }
}

choice! {
    /// What the touch screen shows: carts over the slot, a wall of boxes, or a list of names
    /// (the selected game's box is on the top screen in every view).
    View {
        Shelf = "shelf", "Carts";
        Grid = "grid", "Cover grid";
        List = "list", "List";
        ListCovers = "list-covers", "List with covers";
    }
}

choice! {
    /// What a SNES cart looks like: the US one, or the Super Famicom's (Japan, Europe and
    /// everywhere else), or whichever the game's region had.
    SnesCart {
        Region = "region", "Match the game's region";
        Us = "us", "US";
        Famicom = "famicom", "Super Famicom";
    }
}

choice! {
    Sort {
        Name = "name", "Name";
        Recent = "recent", "Recently played";
        Rating = "rating", "Rating";
    }
}

choice! {
    /// Which games a page shows.
    Show {
        All = "all", "All games";
        Favourites = "favourites", "Favourites";
    }
}

choice! {
    /// RetroAchievements: hardcore (no save states, rewinds or cheats; the achievements
    /// count for the site's main leaderboards) or softcore, shown as Casual (the service's own
    /// name for it). The emulator decides what it can honour: one with no hardcore support
    /// plays softcore whatever is set.
    Achievements {
        Softcore = "softcore", "Casual";
        Hardcore = "hardcore", "Hardcore";
    }
}

choice! {
    /// How soon a held D-pad starts repeating, and how fast.
    Repeat {
        Slow = "slow", "Slow";
        Normal = "normal", "Normal";
        Fast = "fast", "Fast";
    }
}

choice! {
    /// How long the shelf is left before the screensaver comes on.
    Saver {
        Off = "off", "Off";
        One = "1", "1 min";
        Two = "2", "2 min";
        Five = "5", "5 min";
        Ten = "10", "10 min";
    }
}

choice! {
    /// What the screensaver shows: the logo drifting (a few frames a second), the logo still
    /// (moved now and then), or nothing, the screens' backlights off.
    SaverLook {
        Animated = "animated", "Animated";
        Still = "still", "Still";
        Off = "off", "Screens off";
    }
}

impl Saver {
    /// The time left alone before it comes on, if it ever does.
    pub fn after(self) -> Option<std::time::Duration> {
        let mins = match self {
            Saver::Off => return None,
            Saver::One => 1,
            Saver::Two => 2,
            Saver::Five => 5,
            Saver::Ten => 10,
        };
        Some(std::time::Duration::from_secs(mins * 60))
    }
}

impl Case {
    /// Whether a game with this four-character code gets a black case.
    pub fn black(self, code: &str) -> bool {
        match self {
            Case::Clear => false,
            Case::Black => true,
            // The code's last letter is the region. These are the European and Australian ones.
            Case::Region => !code
                .as_bytes()
                .get(3)
                .is_some_and(|r| b"PDFSIHUXYZVW".contains(r)),
        }
    }
}

impl Repeat {
    /// The wait before the first repeat, and between the rest, in milliseconds.
    pub fn millis(self) -> (u64, u64) {
        match self {
            Repeat::Slow => (450, 120),
            Repeat::Normal => (340, 85),
            Repeat::Fast => (250, 50),
        }
    }
}

/// DSlot's own settings, by key.
pub fn builtin(key: &str) -> Option<Def> {
    let def = |label: &str, kind, levels, default: &str| Def {
        key: key.to_owned(),
        label: label.to_owned(),
        kind,
        levels,
        default: Some(Value::from(default)),
    };
    Some(match key {
        VIEW => def(
            "View",
            View::choices(),
            Levels::SYSTEM_AND_CONSOLE,
            View::Shelf.key(),
        ),
        SORT => def(
            "Sort by",
            Sort::choices(),
            Levels::SYSTEM_AND_CONSOLE,
            Sort::Name.key(),
        ),
        SHOW => def(
            "Show",
            Show::choices(),
            Levels::SYSTEM_AND_CONSOLE,
            Show::All.key(),
        ),
        CASE => def(
            "Game case",
            Case::choices(),
            Levels::SYSTEM_AND_CONSOLE,
            Case::Region.key(),
        ),
        // Only the SNES page has it: a SNES cart's look.
        SNES_CART => def(
            "SNES carts",
            SnesCart::choices(),
            Levels {
                system: false,
                console: true,
                game: false,
            },
            SnesCart::Region.key(),
        ),
        REPEAT => def(
            "Button repeat",
            Repeat::choices(),
            Levels::SYSTEM,
            Repeat::Normal.key(),
        ),
        SAVER => def(
            "Screensaver",
            Saver::choices(),
            Levels::SYSTEM,
            Saver::Two.key(),
        ),
        SAVER_LOOK => def(
            "Screensaver style",
            SaverLook::choices(),
            Levels::SYSTEM,
            SaverLook::Animated.key(),
        ),
        ACHIEVEMENTS => def(
            "Mode",
            Achievements::choices(),
            Levels::SYSTEM,
            Achievements::Softcore.key(),
        ),
        SOUNDS => Def {
            default: Some(Value::Boolean(true)),
            ..def("Sounds", Kind::Toggle, Levels::SYSTEM, "")
        },
        SHOW_EMPTY => Def {
            default: Some(Value::Boolean(false)),
            ..def(
                "Show systems with no games",
                Kind::Toggle,
                Levels::SYSTEM,
                "",
            )
        },
        _ => return None,
    })
}

pub const VIEW: &str = "view";
pub const SORT: &str = "sort";
pub const SHOW: &str = "show";
pub const CASE: &str = "case";
pub const SNES_CART: &str = "snes_cart";
pub const REPEAT: &str = "repeat";
pub const SOUNDS: &str = "sounds";
/// Whether a console with no games on either card has a page (and a tab) all the same.
pub const SHOW_EMPTY: &str = "show_empty_systems";
pub const SAVER: &str = "screensaver";
pub const SAVER_LOOK: &str = "screensaver_look";
/// RetroAchievements: hardcore or softcore (achievements/).
pub const ACHIEVEMENTS: &str = "achievements";
/// The emulator a console's or a game's games start in, by its id.
pub const EMULATOR: &str = "emulator";
/// Words added to an emulator's command line, under the emulator's id: `dsperate.args`.
pub const ARGS: &str = "args";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping() {
        let view = builtin(VIEW).unwrap();
        assert_eq!(view.step(None, 1), Value::from("grid"));
        assert_eq!(
            view.step(Some(&Value::from("grid")), 1),
            Value::from("list")
        );
        assert_eq!(
            view.step(Some(&Value::from("list-covers")), 1),
            Value::from("shelf")
        );
        assert_eq!(view.show(&Value::from("list-covers")), "List with covers");
        assert_eq!(view.show(&Value::from("grid")), "Cover grid");
        assert!(view.accepts(&Value::from("shelf")) && !view.accepts(&Value::from("wall")));

        let sounds = builtin(SOUNDS).unwrap();
        assert_eq!(sounds.step(None, 1), Value::Boolean(false));
        assert_eq!(sounds.show(&Value::Boolean(true)), "On");

        let skip = Def {
            key: "x.frameskip".into(),
            label: "Frame skip".into(),
            kind: Kind::Number {
                min: 0,
                max: 3,
                step: 1,
                unit: String::new(),
            },
            levels: Levels::CONSOLE_AND_GAME,
            default: None,
        };
        assert_eq!(skip.step(None, 1), Value::Integer(0));
        assert_eq!(skip.step(Some(&Value::Integer(3)), 1), Value::Integer(3));
        assert_eq!(skip.step(Some(&Value::Integer(2)), -1), Value::Integer(1));
        assert!(!skip.accepts(&Value::Integer(9)));
    }
}
