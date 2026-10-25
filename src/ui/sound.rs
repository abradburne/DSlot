//! The clicks of a game card going into and out of a DS slot, recorded on the real thing:
//! 48 kHz, 16-bit, mono. Each clip has two clicks: the push, then the latch (or the pop).
//!
//! On the DS+ sound goes through PulseAudio (stock) or PipeWire's (nibOS), and SDL only
//! reaches the stock one by way of an ALSA shim whose small buffers underrun on every period,
//! each one a gap in the click. So a clip is handed whole to `pacat`, PulseAudio's own player,
//! which also lets it finish after the shelf has gone. Without `pacat` (a desktop), SDL plays
//! it: the device is opened for each click and closed again once it has played.

use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::AudioSubsystem;
use std::io::Write;
use std::process::{Command, Stdio};

pub const INSERT: &[u8] = include_bytes!("../../assets/insert.pcm");
pub const EJECT: &[u8] = include_bytes!("../../assets/eject.pcm");
const CLIP_HZ: usize = 48_000;
/// Seconds into each clip at which the first click lands.
pub const CLICK_LEAD: f32 = 0.025;
/// Seconds from the push click to the latch click (insert) and to the pop (eject).
pub const LATCH_GAP: f32 = 0.228;
pub const POP_GAP: f32 = 0.262;
/// Silence played ahead of the eject clip. The game has only just let go of the sound card,
/// and a device that is still starting up swallows the first click.
pub const EJECT_WARMUP: f32 = 0.35;

/// What SDL's audio device is opened at, where SDL is what plays the clips.
const DEVICE_HZ: usize = 44_100;
/// How long SDL's queue must have been empty before the device's own buffer has played out.
const DRAIN_SECS: f32 = 0.25;
/// How long an idle SDL device is kept open.
const CLOSE_SECS: f32 = 0.4;

pub struct Sound {
    /// The Sounds setting.
    pub on: bool,
    /// Cleared the first time `pacat` cannot be started.
    pacat: bool,
    /// SDL's audio, opened once the shelf is on screen (slow to start cold).
    console: Option<AudioSubsystem>,
    queue: Option<AudioQueue<i16>>,
    /// Seconds SDL's queue has been empty.
    idle: f32,
}

impl Sound {
    pub fn new(on: bool) -> Sound {
        Sound {
            on,
            pacat: true,
            console: None,
            queue: None,
            idle: 0.0,
        }
    }

    /// SDL's audio, for when there is no `pacat`. Sound is optional: with no audio device the
    /// shelf is simply silent.
    pub fn set_audio(&mut self, console: Option<AudioSubsystem>) {
        self.console = console;
    }

    /// Plays `pcm` behind `lead` seconds of silence.
    pub fn play_after(&mut self, lead: f32, pcm: &[u8]) {
        if !self.on {
            return;
        }
        if self.pacat {
            match pacat(lead, pcm) {
                Ok(()) => return,
                Err(_) => self.pacat = false,
            }
        }
        if self.queue.is_none() {
            let spec = AudioSpecDesired {
                freq: Some(DEVICE_HZ as i32),
                channels: Some(2),
                samples: Some(2048),
            };
            self.queue = self
                .console
                .as_ref()
                .and_then(|a| a.open_queue::<i16, _>(None, &spec).ok());
        }
        if let Some(queue) = &self.queue {
            let _ = queue.queue_audio(&resample(lead, pcm));
            queue.resume();
            self.idle = 0.0;
        }
    }

    /// True once everything queued has reached the speaker (or there is no sound at all).
    /// What `pacat` is playing needs no waiting for.
    pub fn drained(&self) -> bool {
        self.queue
            .as_ref()
            .is_none_or(|q| q.size() == 0 && self.idle > DRAIN_SECS)
    }

    /// Closes SDL's device once it has been idle a while.
    pub fn tick(&mut self, dt: f32) {
        if self.queue.as_ref().is_some_and(|q| q.size() == 0) {
            self.idle += dt;
            if self.idle > CLOSE_SECS {
                self.queue = None;
            }
        }
    }
}

/// A 48 kHz mono clip as SDL's device takes it: `lead` seconds of silence, then the clip at
/// the device's rate, the same on both channels.
fn resample(lead: f32, pcm: &[u8]) -> Vec<i16> {
    let clip: Vec<f32> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32)
        .collect();
    let mut samples = vec![0i16; (lead * DEVICE_HZ as f32) as usize * 2];
    if clip.is_empty() {
        return samples;
    }
    for i in 0..clip.len() * DEVICE_HZ / CLIP_HZ {
        let at = i as f32 * CLIP_HZ as f32 / DEVICE_HZ as f32;
        let (a, b) = (
            clip[at as usize],
            clip[(at as usize + 1).min(clip.len() - 1)],
        );
        samples.extend([(a + (b - a) * at.fract()) as i16; 2]);
    }
    samples
}

/// Plays 48 kHz mono `pcm` behind `lead` seconds of silence through PulseAudio's own player.
fn pacat(lead: f32, pcm: &[u8]) -> std::io::Result<()> {
    let rate = format!("--rate={CLIP_HZ}");
    let args = [
        "--playback",
        "--raw",
        "--format=s16le",
        "--channels=1",
        &rate,
        "--latency-msec=30",
        "--client-name=DSlot",
    ];
    let mut child = Command::new("pacat")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    let mut data = vec![0u8; (lead * CLIP_HZ as f32) as usize * 2];
    data.extend_from_slice(pcm);
    // Fed from its own thread: the clip is larger than a pipe holds. `pacat` plays out what
    // it was given and exits once its input is closed.
    std::thread::spawn(move || {
        if let Some(mut pipe) = child.stdin.take() {
            let _ = pipe.write_all(&data);
        }
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampled() {
        // 0.1 s of silence, then 480 samples (10 ms) at 44.1 kHz on two channels.
        let pcm: Vec<u8> = (0..480i16).flat_map(|i| (i * 10).to_le_bytes()).collect();
        let out = resample(0.1, &pcm);
        assert_eq!(out.len(), 4410 * 2 + 441 * 2);
        assert!(out[..8820].iter().all(|&s| s == 0));
        assert_eq!(out[8820], out[8821]);
        assert_eq!(*out.last().unwrap() / 10, 478);
    }
}
