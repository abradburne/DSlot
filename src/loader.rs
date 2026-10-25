//! Artwork is composed on worker threads, so reading and scaling covers never holds up a frame.
//! The shelf asks for what it wants most; finished images come back to be made into textures.

use crate::art::{self, Shell};
use crate::gfx::{Fonts, Img};
use crate::rom::Game;
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::Scope;

const WORKERS: usize = 2;
/// Jobs handed out at once. Kept small so the next one is always chosen by what is on screen now.
const AT_ONCE: usize = 3;

/// A piece of artwork for the game at this index.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Job {
    /// Cart and full-size box, for the shelf and the top screen.
    Art(usize),
    /// Small box for the cover grid.
    Thumb(usize),
}

pub enum Made {
    Art { cart: Img, cover: Img },
    Thumb(Img),
}

/// What a worker needs to compose artwork, besides the game.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    pub fonts: &'a Fonts,
    pub shell: &'a Shell,
    /// Size of a grid cover in pixels, and where finished ones are kept between runs.
    pub thumb: (usize, usize),
    pub cache: &'a Path,
}

/// A job, its game (the list of games may grow while the job is out), how the page's artwork
/// looks, and the generation it was asked for in.
type Task = (Job, Game, art::Looks, u32);

pub struct Loader {
    tasks: Sender<Task>,
    made: Receiver<(Job, u32, Made)>,
    out: HashSet<Job>,
    /// Bumped when artwork already asked for is no longer wanted as it was.
    generation: u32,
}

impl Loader {
    /// The workers stop when the loader is dropped.
    pub fn start<'scope, 'env: 'scope>(
        scope: &'scope Scope<'scope, 'env>,
        source: Source<'env>,
    ) -> Loader {
        let (sender, tasks) = channel::<Task>();
        let tasks = Arc::new(Mutex::new(tasks));
        let (done, made) = channel();
        for _ in 0..WORKERS {
            let (tasks, done) = (tasks.clone(), done.clone());
            scope.spawn(move || loop {
                // The lock is held while waiting, which is fine: an idle worker has nothing else to do.
                let Ok((job, game, looks, generation)) = tasks.lock().unwrap().recv() else {
                    return;
                };
                let made = match job {
                    Job::Art(_) => {
                        let (cart, cover) = art::art(source.shell, &game, source.fonts, looks);
                        Made::Art { cart, cover }
                    }
                    Job::Thumb(_) => Made::Thumb(art::thumb(
                        &game,
                        source.fonts,
                        looks.black(&game),
                        source.thumb,
                        source.cache,
                    )),
                };
                if done.send((job, generation, made)).is_err() {
                    return;
                }
            });
        }
        Loader {
            tasks: sender,
            made,
            out: HashSet::new(),
            generation: 0,
        }
    }

    /// Whether another job can be handed out now.
    pub fn free(&self) -> bool {
        self.out.len() < AT_ONCE
    }

    pub fn idle(&self) -> bool {
        self.out.is_empty()
    }

    pub fn asked(&self, job: Job) -> bool {
        self.out.contains(&job)
    }

    pub fn ask(&mut self, job: Job, game: &Game, looks: art::Looks) {
        if self.out.insert(job) {
            let _ = self.tasks.send((job, game.clone(), looks, self.generation));
        }
    }

    /// Artwork finished since the last call.
    pub fn take(&mut self) -> Vec<(Job, Made)> {
        let mut ready = Vec::new();
        for (job, generation, made) in self.made.try_iter() {
            if generation == self.generation {
                self.out.remove(&job);
                ready.push((job, made));
            }
        }
        ready
    }

    /// Disown everything asked for so far; its results are thrown away when they arrive.
    pub fn forget(&mut self) {
        self.generation += 1;
        self.out.clear();
    }
}
