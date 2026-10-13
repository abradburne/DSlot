//! The user's choices, kept in a small key=value file next to the binary.

use std::fs;
use std::path::PathBuf;

/// A setting with a few named values: its key in the file and its label on the settings page.
macro_rules! choice {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $key:literal, $label:literal;)+ }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq)]
        pub enum $name { $($variant),+ }

        impl $name {
            const ALL: &'static [$name] = &[$($name::$variant),+];

            fn key(self) -> &'static str {
                match self { $($name::$variant => $key),+ }
            }

            pub fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            fn parse(key: &str) -> Option<$name> {
                Self::ALL.iter().copied().find(|v| v.key() == key)
            }

            /// The value `dir` steps on, wrapping round.
            pub fn step(self, dir: i32) -> $name {
                let i = Self::ALL.iter().position(|v| *v == self).unwrap_or(0) as i32;
                Self::ALL[(i + dir).rem_euclid(Self::ALL.len() as i32) as usize]
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
    /// What the touch screen shows: carts over the slot, or a wall of boxes.
    View {
        Shelf = "shelf", "Carts";
        Grid = "grid", "Cover grid";
    }
}

choice! {
    Sort {
        Name = "name", "Name";
        Recent = "recent", "Recently played";
        Rating = "rating", "Rating";
    }
}

impl Case {
    /// Whether a game with this four-character code gets a black case.
    pub fn black(self, code: &str) -> bool {
        match self {
            Case::Clear => false,
            Case::Black => true,
            // The code's last letter is the region. These are the European and Australian ones.
            Case::Region => !code.as_bytes().get(3).is_some_and(|r| b"PDFSIHUXYZVW".contains(r)),
        }
    }
}

pub struct Settings {
    file: PathBuf,
    pub case: Case,
    pub view: View,
    pub sort: Sort,
    pub faves_only: bool,
    pub sound: bool,
}

impl Settings {
    pub fn load(file: PathBuf) -> Settings {
        let mut s = Settings { file, case: Case::Region, view: View::Shelf, sort: Sort::Name, faves_only: false, sound: true };
        for line in fs::read_to_string(&s.file).unwrap_or_default().lines() {
            match line.split_once('=') {
                Some(("case", v)) => s.case = Case::parse(v).unwrap_or(s.case),
                Some(("view", v)) => s.view = View::parse(v).unwrap_or(s.view),
                Some(("sort", v)) => s.sort = Sort::parse(v).unwrap_or(s.sort),
                Some(("show", v)) => s.faves_only = v == "favourites",
                Some(("sound", v)) => s.sound = v != "off",
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let out = format!(
            "case={}\nview={}\nsort={}\nshow={}\nsound={}\n",
            self.case.key(),
            self.view.key(),
            self.sort.key(),
            if self.faves_only { "favourites" } else { "all" },
            if self.sound { "on" } else { "off" },
        );
        if let Err(e) = fs::write(&self.file, out) {
            eprintln!("settings: cannot write {}: {e}", self.file.display());
        }
    }
}
