//! How long the device stays asleep on battery before it powers itself off: nibOS's `nib sleep`
//! (`/usr/lib/nibos/nib-sleep`, its sleep hook `system-sleep/nibos-sleep-off`). Read as anyone,
//! set as root through `sudo -n`, which nibOS's `nibos-sleep` sudoers lets the wheel group do.
//! Where the script is missing (another system) the System page leaves the row out.

use std::process::Command;

/// nibOS's script.
pub const NIB_SLEEP: &str = "/usr/lib/nibos/nib-sleep";

/// What the row steps through, in minutes; `None` is never.
pub const CHOICES: [Option<u32>; 7] = [
    Some(15),
    Some(30),
    Some(60),
    Some(120),
    Some(360),
    Some(1440),
    None,
];

/// The setting as `nib sleep` has it now, or `None` where it can't be read.
pub fn read() -> Option<Option<u32>> {
    let out = Command::new(NIB_SLEEP).output().ok()?;
    if !out.status.success() {
        return None;
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// `off-after: 60 min` or `off-after: never`.
pub fn parse(out: &str) -> Option<Option<u32>> {
    let v = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("off-after:"))?
        .trim();
    if v == "never" {
        return Some(None);
    }
    v.strip_suffix("min")?.trim().parse().ok().map(Some)
}

/// The next choice from `now` in direction `dir`. A time set elsewhere that isn't one of the
/// choices steps from the first choice past it.
pub fn step(now: Option<u32>, dir: i32) -> Option<u32> {
    let at = CHOICES
        .iter()
        .position(|c| *c == now)
        .map(|i| i as i32)
        .unwrap_or_else(|| {
            let past = CHOICES
                .iter()
                .position(|c| c.zip(now).is_some_and(|(c, n)| c > n))
                .unwrap_or(CHOICES.len() - 1) as i32;
            // Stepping up from between two choices lands on the one past; down, the one before.
            if dir > 0 {
                past - 1
            } else {
                past
            }
        });
    CHOICES[(at + dir).rem_euclid(CHOICES.len() as i32) as usize]
}

/// "After 1 hour", "After 30 min", "Never".
pub fn label(v: Option<u32>) -> String {
    match v {
        None => "Never".into(),
        Some(1440) => "After 1 day".into(),
        Some(60) => "After 1 hour".into(),
        Some(m) if m % 60 == 0 => format!("After {} hours", m / 60),
        Some(m) => format!("After {m} min"),
    }
}

/// Sets it through `sudo -n nib sleep off-after`, kept across restarts and updates.
pub fn set(v: Option<u32>) -> std::io::Result<()> {
    let arg = v.map_or("never".to_owned(), |m| m.to_string());
    let out = Command::new("sudo")
        .args(["-n", NIB_SLEEP, "off-after", &arg])
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nib_sleep() {
        assert_eq!(parse("off-after: 60 min\n"), Some(Some(60)));
        assert_eq!(parse("off-after: never\n"), Some(None));
        assert_eq!(parse("something else\n"), None);
    }

    #[test]
    fn steps_round_the_choices() {
        assert_eq!(step(Some(60), 1), Some(120));
        assert_eq!(step(Some(60), -1), Some(30));
        assert_eq!(step(None, 1), Some(15));
        assert_eq!(step(Some(15), -1), None);
        // Set by hand to 45: up is 60, down is 30.
        assert_eq!(step(Some(45), 1), Some(60));
        assert_eq!(step(Some(45), -1), Some(30));
        // Past the last time: up is never, down is a day.
        assert_eq!(step(Some(1500), 1), None);
        assert_eq!(step(Some(1500), -1), Some(1440));
    }

    #[test]
    fn labels() {
        assert_eq!(label(Some(15)), "After 15 min");
        assert_eq!(label(Some(60)), "After 1 hour");
        assert_eq!(label(Some(360)), "After 6 hours");
        assert_eq!(label(Some(1440)), "After 1 day");
        assert_eq!(label(None), "Never");
    }
}
