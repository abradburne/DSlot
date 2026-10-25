//! The games on a page, in the order the shelf shows them, and moving through them.

use crate::rom::{Console, Game};
use crate::settings::Sort;
use crate::stats::Stats;
use std::collections::HashMap;
use std::path::PathBuf;

/// What a page shows.
#[derive(Clone, Copy)]
pub struct Filter {
    pub console: Console,
    pub sort: Sort,
    pub faves_only: bool,
}

/// The games on the page, in order, as indices into `games`.
pub fn order(games: &[Game], stats: &Stats, filter: Filter) -> Vec<usize> {
    let shown = |i: &usize| {
        let g = &games[*i];
        g.console == filter.console && !g.gone && (!filter.faves_only || stats.fave(&g.path))
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
    // On the Ports page, PortMaster's tile first, however the ports are sorted.
    if filter.console == Console::Ports {
        order.sort_by_key(|&i| !games[i].is_app());
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

    #[test]
    fn letters() {
        let games = games();
        let ds = [0, 1, 3, 4];
        assert_eq!(letter_jump(&games, &ds, 0, 1), 2);
        assert_eq!(letter_jump(&games, &ds, 2, -1), 1);
        assert_eq!(letter_jump(&games, &ds, 3, 1), 3);
        assert_eq!(letter_jump(&games, &ds, 0, -1), 0);
    }
}
