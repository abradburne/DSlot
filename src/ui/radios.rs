//! The menu's Wi-Fi and Bluetooth pages' worker: nibkit's `net::radios` (a thread of its own
//! that talks to iwd and BlueZ and keeps a snapshot for the pages), on nibOS. Elsewhere
//! [`Radios::start`] gives `None` and the pages aren't there.

#[cfg(feature = "nibos")]
pub use nibkit::net::radios::*;

#[cfg(not(feature = "nibos"))]
pub use stub::*;

#[cfg(not(feature = "nibos"))]
mod stub {
    //! The stock OS: the pages' types, and no radios to talk to.
    #![allow(dead_code)]

    #[derive(Clone, Debug, Default)]
    pub struct Snapshot {
        pub wifi: Option<WifiNow>,
        pub bluetooth: Option<BluetoothNow>,
        pub busy: Option<String>,
        pub said: Option<String>,
        pub asking: Option<Ask>,
    }
    #[derive(Clone, Debug, Default)]
    pub struct WifiNow {
        pub joined: Option<(String, u8)>,
        pub state: String,
        pub scanning: bool,
        pub networks: Vec<Net>,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Net {
        pub name: String,
        pub bars: u8,
        pub known: bool,
        pub connected: bool,
        pub locked: bool,
        pub secured: bool,
        pub unsupported: bool,
    }
    #[derive(Clone, Debug, Default)]
    pub struct BluetoothNow {
        pub powered: bool,
        pub discovering: bool,
        pub devices: Vec<Gadget>,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Gadget {
        pub name: String,
        pub address: String,
        pub kind: &'static str,
        pub paired: bool,
        pub connected: bool,
        pub battery: Option<u8>,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Ask {
        Pin,
        Passkey,
        Confirm(u32),
        Show(String),
        Allow,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Reply {
        Yes,
        No,
        Code(String),
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Do {
        Scan,
        Join(String, Option<String>),
        Leave,
        Forget(String),
        Power(bool),
        Look(bool),
        Pair(String),
        Connect(String),
        Disconnect(String),
        Remove(String),
    }

    pub struct Radios;

    impl Radios {
        pub fn start() -> Option<Radios> {
            None
        }
        pub fn now(&self) -> Snapshot {
            Snapshot::default()
        }
        pub fn ask(&self, _what: Do) {}
        pub fn reply(&self, _reply: Reply) {}
    }

    /// A device's value on a row.
    pub fn device_value(_g: &Gadget) -> nibkit::menu::Value {
        nibkit::menu::Value::None
    }
}

/// A device's value on a row, as every app's Connections page shows it.
#[cfg(feature = "nibos")]
pub use nibkit::menu::connections::device_value;
