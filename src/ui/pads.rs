//! The game controllers the shelf reads, kept open for as long as they are there, and opened
//! again when one comes back. On nibOS the device's own gamepad is nibos-input's: a restart of
//! it (an update of nibos-hw, or systemd starting it again after a crash) takes that gamepad
//! away and makes a new one, and a shelf that kept only the old one would hear nothing but the
//! keys nibos-input handles itself (power). SDL says when a controller comes and goes; this
//! follows it.

use sdl2::controller::GameController;
use sdl2::event::Event;
use sdl2::GameControllerSubsystem;

/// The controllers open now.
pub struct Pads {
    sub: GameControllerSubsystem,
    open: Vec<GameController>,
}

impl Pads {
    /// Every controller there is now, with nibkit's mappings for the device's own. SDL opens
    /// every input device to ask what it is, so this is done once (after the first frame), and
    /// later ones are opened as SDL reports them.
    pub fn open(sub: GameControllerSubsystem) -> Result<Pads, String> {
        for mappings in nibkit::input::CONTROLLER_MAPPINGS {
            let _ = sub.load_mappings(mappings);
        }
        let mut pads = Pads {
            open: Vec::new(),
            sub,
        };
        for index in 0..pads.sub.num_joysticks()? {
            if pads.sub.is_game_controller(index) {
                pads.add(index);
            }
        }
        Ok(pads)
    }

    /// Keeps up with a controller coming or going. Other events are left alone.
    pub fn event(&mut self, event: &Event) {
        match *event {
            Event::ControllerDeviceAdded { which, .. } => {
                if self.add(which) {
                    eprintln!(
                        "dslot: controller back: {}",
                        self.open.last().map_or(String::new(), |p| p.name())
                    );
                }
            }
            Event::ControllerDeviceRemoved { which, .. } => {
                let before = self.open.len();
                self.open.retain(|p| p.instance_id() != which);
                if self.open.len() < before {
                    eprintln!("dslot: controller gone (instance {which})");
                }
            }
            _ => {}
        }
    }

    /// Opens the controller at joystick index `index` unless it is open already (SDL also
    /// reports, as added, the ones there at the start). Whether it was new.
    fn add(&mut self, index: u32) -> bool {
        let Ok(pad) = self.sub.open(index) else {
            return false;
        };
        if self
            .open
            .iter()
            .any(|p| p.instance_id() == pad.instance_id())
        {
            return false;
        }
        self.open.push(pad);
        true
    }
}
