//! The games on a page, in the order the shelf shows them, and moving through them.

use crate::rom::{Console, Game, HomeSection};
use crate::settings::Sort;
use crate::stats::Stats;
use std::collections::HashMap;
use std::path::PathBuf;

/// Maximum number of recently played games kept on the cross-console Home shelf.
pub const HOME_RECENT_MAX: usize = 50;

/// The section of a sectioned page's games that their list puts in no group of its own.
pub const OTHER: &str = "Other";

/// What a page shows.
#[derive(Clone, Copy)]
pub struct Filter<'a> {
    pub console: Console,
    pub sort: Sort,
    pub faves_only: bool,
    /// One section of the page's games alone (`Game::group`: an arcade board), or all of them.
    pub group: Option<&'a str>,
    /// The folder of games open (`Game::sub`): its games and folders alone. Favourites alone and
    /// a section show every game of theirs, in whatever folder, and no folders.
    pub dir: &'a str,
}

impl Filter<'_> {
    /// Whether the page shows its games whatever folder they are in, and no folders: favourites
    /// alone, or one section.
    pub fn flat(&self) -> bool {
        self.faves_only || self.group.is_some()
    }
}

/// The games on the page, in order, as indices into `games`.
pub fn order(games: &[Game], stats: &Stats, filter: Filter) -> Vec<usize> {
    let shown = |i: &usize| {
        let g = &games[*i];
        let flat = filter.flat();
        g.console == filter.console
            && !g.gone
            && if flat { !g.folder } else { g.sub == filter.dir }
            && (!filter.faves_only || stats.fave(&g.path))
            && filter
                .group
                .is_none_or(|group| g.group == group || (group == OTHER && g.group.is_empty()))
    };
    let mut order: Vec<usize> = (0..games.len()).filter(shown).collect();
    // By name first: games found since DSlot started are at the end of `games`. The other sorts
    // are stable, so games that tie stay in name order.
    order.sort_by_cached_key(|&i| games[i].name().to_lowercase());
    match filter.sort {
        Sort::Name => {}
        Sort::Recent => order.sort_by_key(|&i| std::cmp::Reverse(stats.last(&games[i].path))),
        Sort::Rating => order.sort_by_key(|&i| std::cmp::Reverse(stats.rating(&games[i].path))),
    }
    // Folders of games before the games, by name, however the games are sorted.
    order.sort_by_key(|&i| !games[i].folder);
    // On the Ports page, PortMaster's tile first, however the ports are sorted.
    if filter.console == Console::Ports {
        order.sort_by_key(|&i| !games[i].is_app());
    }
    order
}

/// The games on the cross-console Home shelf, in order, as indices into `games`.
///
/// - `HomeSection::Recent`: games played at least once across all consoles (excluding Apps and
///   folders), newest first, capped at [`HOME_RECENT_MAX`].
/// - `HomeSection::Favourites`: all favourited games across all consoles, sorted by `sort`.
/// - `HomeSection::Rating`: all rated games across all consoles, highest rating first.
/// - `HomeSection::Name`: every game across all consoles (excluding Apps and folders), A–Z.
pub fn home_order(games: &[Game], stats: &Stats, section: HomeSection, sort: Sort) -> Vec<usize> {
    let mut order: Vec<usize> = (0..games.len())
        .filter(|&i| {
            let g = &games[i];
            !g.gone
                && !g.folder
                && g.console != Console::Apps
                && match section {
                    HomeSection::Recent => stats.last(&g.path) > 0,
                    HomeSection::Favourites => stats.fave(&g.path),
                    HomeSection::Rating => stats.rating(&g.path) > 0,
                    HomeSection::Name => true,
                }
        })
        .collect();
    order.sort_by_cached_key(|&i| games[i].name().to_lowercase());
    match section {
        HomeSection::Recent => {
            order.sort_by_key(|&i| std::cmp::Reverse(stats.last(&games[i].path)));
            order.truncate(HOME_RECENT_MAX);
        }
        HomeSection::Favourites => match sort {
            Sort::Name => {}
            Sort::Recent => order.sort_by_key(|&i| std::cmp::Reverse(stats.last(&games[i].path))),
            Sort::Rating => order.sort_by_key(|&i| std::cmp::Reverse(stats.rating(&games[i].path))),
        },
        HomeSection::Rating => {
            order.sort_by_key(|&i| std::cmp::Reverse(stats.rating(&games[i].path)));
        }
        HomeSection::Name => {}
    }
    order
}

/// Brings `games` up to date with `found`, a fresh look in `console`'s folders, without moving
/// a game: one found again stays where it is (and comes back if it had gone), one no longer
/// there is marked gone, and a new one goes on the end. So every index into `games` (the shelf's
/// selection and artwork, a game being played) stays good. Returns how many came and went.
pub fn merge(games: &mut Vec<Game>, console: Console, found: Vec<Game>) -> (usize, usize) {
    let mut found: HashMap<PathBuf, Game> =
        found.into_iter().map(|g| (g.path.clone(), g)).collect();
    let (mut came, mut went) = (0, 0);
    for game in games.iter_mut().filter(|g| g.console == console) {
        match found.remove(&game.path) {
            Some(again) if game.gone => {
                *game = again;
                came += 1;
            }
            // A folder of games found again: how many games are in it may have changed.
            Some(again) if game.folder => game.title = again.title,
            Some(_) => {}
            None if !game.gone => {
                game.gone = true;
                went += 1;
            }
            None => {}
        }
    }
    let mut new: Vec<Game> = found.into_values().collect();
    new.sort_by_key(|g| g.name().to_lowercase());
    came += new.len();
    games.extend(new);
    (came, went)
}

/// The next game in `order` from `sel` whose name starts with another letter, in direction
/// `dir`; the end of the list if there is none.
pub fn letter_jump(games: &[Game], order: &[usize], sel: usize, dir: i32) -> usize {
    let initial = |i: usize| {
        games[order[i]]
            .name()
            .chars()
            .next()
            .map(|c| c.to_ascii_lowercase())
    };
    let mut i = sel as i32;
    loop {
        let next = i + dir;
        if next < 0 || next >= order.len() as i32 {
            return i as usize;
        }
        i = next;
        if initial(i as usize) != initial(sel) {
            return i as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::path::PathBuf;

    fn game(console: Console, name: &str) -> Game {
        let path = PathBuf::from(format!("/r/{}/{name}.rom", console.folder()));
        Game {
            console,
            path,
            stem: name.into(),
            code: String::new(),
            title: vec![name.into()],
            icon: None,
            cover: None,
            gone: false,
            group: String::new(),
            folder: false,
            sub: String::new(),
        }
    }

    fn games() -> Vec<Game> {
        // Sorted by name, as rom::scan leaves them.
        vec![
            game(Console::Ds, "Advance"),
            game(Console::Ds, "Animal"),
            game(Console::Gba, "Banjo"),
            game(Console::Ds, "Bomber"),
            game(Console::Ds, "Castle"),
        ]
    }

    /// A sectioned page shows one section's games alone: its group's, and for Other also the
    /// games their list puts in none.
    #[test]
    fn one_section_alone() {
        let set = |name: &str, group: &str| Game {
            group: group.into(),
            folder: false,
            sub: String::new(),
            ..game(Console::Arcade, name)
        };
        let games = vec![
            set("Metal Slug", "Neo Geo"),
            set("Street Fighter II", "CPS-1"),
            set("Gridlee", "Other"),
            set("Unlisted", ""),
            set("Samurai Shodown", "Neo Geo"),
        ];
        let dir = TempDir::new("sections");
        let stats = Stats::load(dir.path().join("stats.tsv"));
        let arcade = Filter {
            console: Console::Arcade,
            sort: Sort::Name,
            faves_only: false,
            group: None,
            dir: "",
        };
        assert_eq!(order(&games, &stats, arcade).len(), 5);
        let neo = Filter {
            group: Some("Neo Geo"),
            ..arcade
        };
        assert_eq!(order(&games, &stats, neo), [0, 4]);
        let other = Filter {
            group: Some(OTHER),
            ..arcade
        };
        assert_eq!(order(&games, &stats, other), [2, 3]);
    }

    /// PortMaster's tile (an app's folder, not a port's script) heads the Ports page whatever
    /// the sort, and is started as an app; the ports are not.
    #[test]
    fn get_more_ports_first() {
        let port = |name: &str| Game {
            path: PathBuf::from(format!("/r/PORTS/{name}.sh")),
            ..game(Console::Ports, name)
        };
        let tile = Game {
            path: PathBuf::from("/usr/share/dslot/apps/portmaster"),
            title: vec!["Get more ports".into(), "PortMaster".into()],
            ..game(Console::Ports, "portmaster")
        };
        let games = vec![port("Celeste"), port("Augustus"), tile, port("Zork")];
        assert!(games[2].is_app() && !games[0].is_app());
        let dir = TempDir::new("ports-order");
        let mut stats = Stats::load(dir.path().join("stats.tsv"));
        stats.record(&games[3].path, 60);
        for sort in [Sort::Name, Sort::Recent, Sort::Rating] {
            let filter = Filter {
                console: Console::Ports,
                sort,
                faves_only: false,
                group: None,
                dir: "",
            };
            assert_eq!(order(&games, &stats, filter)[0], 2, "{sort:?}");
        }
    }

    #[test]
    fn page_order() {
        let games = games();
        let dir = TempDir::new("library");
        let mut stats = Stats::load(dir.path().join("stats.tsv"));
        let ds = Filter {
            console: Console::Ds,
            sort: Sort::Name,
            faves_only: false,
            group: None,
            dir: "",
        };
        assert_eq!(order(&games, &stats, ds), [0, 1, 3, 4]);
        assert_eq!(
            order(
                &games,
                &stats,
                Filter {
                    console: Console::Gba,
                    ..ds
                }
            ),
            [2]
        );

        stats.set_rating(&games[3].path, 4);
        stats.set_rating(&games[1].path, 2);
        stats.toggle_fave(&games[4].path);
        // Rated first, best first; the rest in name order.
        assert_eq!(
            order(
                &games,
                &stats,
                Filter {
                    sort: Sort::Rating,
                    ..ds
                }
            ),
            [3, 1, 0, 4]
        );
        assert_eq!(
            order(
                &games,
                &stats,
                Filter {
                    faves_only: true,
                    ..ds
                }
            ),
            [4]
        );
    }

    #[test]
    fn games_come_and_go_in_place() {
        let mut games = games();
        let dir = TempDir::new("library-merge");
        let stats = Stats::load(dir.path().join("stats.tsv"));
        let ds = Filter {
            console: Console::Ds,
            sort: Sort::Name,
            faves_only: false,
            group: None,
            dir: "",
        };
        // "Animal" deleted, "Apple" copied in; the GBA game untouched.
        let found = vec![
            game(Console::Ds, "Advance"),
            game(Console::Ds, "Apple"),
            game(Console::Ds, "Bomber"),
            game(Console::Ds, "Castle"),
        ];
        assert_eq!(merge(&mut games, Console::Ds, found), (1, 1));
        assert_eq!(games.len(), 6);
        assert!(games[1].gone && games[5].name() == "Apple" && !games[2].gone);
        // Every game keeps its index; the new one is shown in name order.
        assert_eq!(order(&games, &stats, ds), [0, 5, 3, 4]);
        // Nothing new: nothing changes.
        let again: Vec<Game> = [0, 5, 3, 4].iter().map(|&i| games[i].clone()).collect();
        assert_eq!(merge(&mut games, Console::Ds, again), (0, 0));
        // "Animal" back: in its old place.
        let back = vec![game(Console::Ds, "Animal")];
        assert_eq!(merge(&mut games, Console::Ds, back), (1, 4));
        assert_eq!(order(&games, &stats, ds), [1]);
        assert_eq!(games.len(), 6);
    }

    /// A page lists the folder of games open, its folders first; favourites alone, every
    /// favourite, wherever it is, and no folders.
    #[test]
    fn folders_of_games() {
        let mut games = vec![
            game(Console::Gb, "Zed"),
            game(Console::Gb, "One"),
            game(Console::Gb, "_chromatic"),
            game(Console::Gb, "Two"),
        ];
        games[1].sub = "_chromatic".into();
        games[2].folder = true;
        games[3].sub = "_chromatic".into();
        let dir = TempDir::new("folders");
        let stats = Stats::load(dir.path().join("stats.tsv"));
        let top = Filter {
            console: Console::Gb,
            sort: Sort::Name,
            faves_only: false,
            group: None,
            dir: "",
        };
        assert_eq!(order(&games, &stats, top), [2, 0]);
        let inside = Filter {
            dir: "_chromatic",
            ..top
        };
        assert_eq!(order(&games, &stats, inside), [1, 3]);
        let faves = Filter {
            faves_only: true,
            ..top
        };
        assert!(faves.flat() && !top.flat());
    }

    #[test]
    fn letters() {
        let games = games();
        let ds = [0, 1, 3, 4];
        assert_eq!(letter_jump(&games, &ds, 0, 1), 2);
        assert_eq!(letter_jump(&games, &ds, 2, -1), 1);
        assert_eq!(letter_jump(&games, &ds, 3, 1), 3);
        assert_eq!(letter_jump(&games, &ds, 0, -1), 0);
    }

    #[test]
    fn home_shelf_orders_recents_and_favourites_across_consoles() {
        let mut games = vec![
            game(Console::Ds, "Zelda"),
            game(Console::PlayStation, "Crash"),
            game(Console::Gba, "Metroid"),
            game(Console::Apps, "Settings"),
            game(Console::Snes, "_hacks"),
        ];
        games[4].folder = true;
        let dir = TempDir::new("home-order");
        let file = dir.path().join("stats.tsv");
        // Crash played most recently (t=200), Zelda older (t=100), Metroid & Zelda favourited,
        // Settings (Apps) played & favourited but excluded from Home games.
        std::fs::write(
            &file,
            format!(
                "{}\t60\t100\t1\t1\t4\n{}\t120\t200\t2\t0\t5\n{}\t0\t0\t0\t1\t3\n{}\t10\t300\t1\t1\t0\n",
                games[0].path.display(),
                games[1].path.display(),
                games[2].path.display(),
                games[3].path.display(),
            ),
        )
        .unwrap();
        let stats = Stats::load(file);

        // Recent: newest played first across consoles, excluding unplayed, folders, and Apps.
        assert_eq!(
            home_order(&games, &stats, HomeSection::Recent, Sort::Name),
            [1, 0]
        );
        // Favourites: alphabetical by default across consoles, excluding Apps.
        assert_eq!(
            home_order(&games, &stats, HomeSection::Favourites, Sort::Name),
            [2, 0]
        );
        // Favourites sorted by rating: Zelda (4★) before Metroid (3★).
        assert_eq!(
            home_order(&games, &stats, HomeSection::Favourites, Sort::Rating),
            [0, 2]
        );
        // Rating: all rated games across consoles, highest first (Crash 5★, Zelda 4★, Metroid 3★).
        assert_eq!(
            home_order(&games, &stats, HomeSection::Rating, Sort::Name),
            [1, 0, 2]
        );
        // Name: all games across consoles A–Z (excluding folders and Apps).
        assert_eq!(
            home_order(&games, &stats, HomeSection::Name, Sort::Name),
            [1, 2, 0]
        );
    }
}
