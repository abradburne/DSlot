//! PortMaster's ports, as PortMaster installs them in `Roms/PORTS`: a script, `<Title>.sh`,
//! and a folder of the port's own, which usually has a `port.json` naming both (`items`), the
//! port's title (`attr.title`) and its picture (`attr.image.screenshot`, or `attr.image`'s
//! first cover), and often a `gameinfo.xml` (`<name>`, `<image>`) and a `screenshot.*` or
//! `cover.*`. A port is listed by its script; its name and picture come from its folder,
//! where there is one that names it, else its name is the script's.

use super::{Console, Game};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The app that is PortMaster (`/usr/share/dslot/apps/portmaster` on nibOS, installed with
/// it): its id.
const PORTMASTER: &str = "portmaster";

/// PortMaster's own tile for the Ports page, "Get more ports" over its name ("Get PortMaster"
/// until it has fetched itself), when it is installed as an app.
/// It is started as the app is (`Game::is_app`), and the ports are looked for again when it
/// exits.
pub fn get_more() -> Option<Game> {
    let found = nibkit::dslot::apps(&[])
        .into_iter()
        .find(|f| f.id == PORTMASTER)?;
    Some(Game {
        console: Console::Ports,
        path: super::app_path(&found),
        stem: found.id.clone(),
        code: String::new(),
        // Its name under it: "Get PortMaster" (its [until] name) until it has fetched itself.
        title: vec!["Get more ports".into(), found.app.shown_name().to_owned()],
        icon: None,
        cover: found.icon.clone(),
        gone: false,
    })
}

/// What a port's folder says about it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Port {
    pub title: Option<String>,
    pub picture: Option<PathBuf>,
}

/// The ports in `dir` (a `Roms/PORTS`), by their script's file name.
pub fn index(dir: &Path) -> HashMap<String, Port> {
    let mut ports = HashMap::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return ports;
    };
    for folder in rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        let json = fs::read_to_string(folder.join("port.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let xml = fs::read_to_string(folder.join("gameinfo.xml")).ok();
        let scripts: Vec<String> = match &json {
            Some(j) => j["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|i| i.as_str())
                .filter(|i| i.to_ascii_lowercase().ends_with(".sh"))
                .map(str::to_owned)
                .collect(),
            None => Vec::new(),
        };
        // Without a port.json, a folder named as a script is taken to be its.
        let scripts = if scripts.is_empty() {
            let name = folder.file_name().unwrap_or_default().to_string_lossy();
            vec![format!("{name}.sh")]
        } else {
            scripts
        };
        let port = Port {
            title: json
                .as_ref()
                .and_then(|j| text(&j["attr"]["title"]))
                .or_else(|| xml.as_deref().and_then(|x| tag(x, "name"))),
            picture: picture(&folder, json.as_ref(), xml.as_deref()),
        };
        for script in scripts {
            ports.insert(script, port.clone());
        }
    }
    ports
}

/// A JSON string with something in it.
fn text(v: &serde_json::Value) -> Option<String> {
    Some(v.as_str()?.trim().to_owned()).filter(|s| !s.is_empty())
}

/// The text of the first `<name>` element in `xml`, its entities undone.
fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    let t = xml[start..end]
        .trim()
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&quot;", "\"");
    Some(t).filter(|s| !s.is_empty())
}

/// The port's picture: the one port.json names, else gameinfo.xml's, else a screenshot or
/// cover in its folder. Only a file that is there, in a format DSlot reads (PNG, JPEG).
fn picture(folder: &Path, json: Option<&serde_json::Value>, xml: Option<&str>) -> Option<PathBuf> {
    let named = json
        .into_iter()
        .flat_map(|j| {
            let image = &j["attr"]["image"];
            [
                text(&image["screenshot"]),
                text(image),
                image["covers"].get(0).and_then(text),
                j["attr"]["covers"].get(0).and_then(text),
            ]
        })
        .flatten()
        .chain(xml.and_then(|x| tag(x, "image")))
        .map(|n| folder.join(n.trim_start_matches("./")));
    let usual = ["screenshot", "cover"].into_iter().flat_map(|n| {
        ["png", "jpg", "jpeg"]
            .into_iter()
            .map(move |e| folder.join(format!("{n}.{e}")))
    });
    named.chain(usual).find(|p| readable(p) && p.is_file())
}

fn readable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn names_and_pictures() {
        let dir = TempDir::new("ports-index");
        let d = dir.path();
        let put = |p: &str, t: &str| {
            let p = d.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, t).unwrap();
        };
        // port.json naming the script, its title and picture.
        put(
            "cave/port.json",
            r#"{"version": 2, "items": ["Cave Story.sh", "cave"], "attr": {"title": "Cave Story", "image": {"screenshot": "screenshot.jpg"}}}"#,
        );
        put("cave/screenshot.jpg", "jpeg");
        // No title or picture in port.json: gameinfo.xml's name, and a cover in the folder.
        put(
            "sonic/port.json",
            r#"{"items": ["Sonic Mania.sh", "sonic"], "attr": {"title": "", "image": {}}}"#,
        );
        put(
            "sonic/gameinfo.xml",
            "<gameList><game><path>./Sonic Mania.sh</path><name>Sonic &amp; Friends</name></game></gameList>",
        );
        put("sonic/cover.png", "png");
        // No port.json: a folder named as the script.
        put("Doom/screenshot.png", "png");
        // A picture named but not there: none.
        put(
            "ghost/port.json",
            r#"{"items": ["Ghost.sh"], "attr": {"title": "Ghost", "image": {"screenshot": "gone.png"}}}"#,
        );

        let ports = index(d);
        assert_eq!(
            ports["Cave Story.sh"],
            Port {
                title: Some("Cave Story".into()),
                picture: Some(d.join("cave/screenshot.jpg")),
            }
        );
        assert_eq!(
            ports["Sonic Mania.sh"].title.as_deref(),
            Some("Sonic & Friends")
        );
        assert_eq!(
            ports["Sonic Mania.sh"].picture,
            Some(d.join("sonic/cover.png"))
        );
        assert_eq!(ports["Doom.sh"].title, None);
        assert_eq!(
            ports["Doom.sh"].picture,
            Some(d.join("Doom/screenshot.png"))
        );
        assert_eq!(ports["Ghost.sh"].picture, None);
    }
}
