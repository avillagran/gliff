//! A tablet, or any device with a web browser, as one more screen of this
//! desktop. Two user units do the work and keep it going while this window
//! is closed: `gliff-tablet-vnc` serves a new screen over VNC on 127.0.0.1,
//! and `gliff-tablet-web` serves the page that shows it on port 6080 (see
//! `docs/tablet.md`). The header's tablet button opens a dialog that picks
//! where the screen stands, starts and stops it, and gives the address to
//! open on the tablet.

use std::io::{Read, Write};
use std::net::{IpAddr, UdpSocket};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;

use crate::monitor::{
    alignment_picker, picked_alignment, picked_side, side_picker, ALIGNMENTS, SIDES,
};

const VNC_UNIT: &str = "gliff-tablet-vnc.service";
const WEB_UNIT: &str = "gliff-tablet-web.service";
const WEB_PORT: u16 = 6080;
/// The screen sizes offered, the first being the default: an iPad's
/// 1024x768 less iOS's status bar and the page's own bar.
const SIZES: [(&str, &str); 4] = [
    ("1024x720", "1024 × 720, iPad from the Home Screen"),
    ("1024x768", "1024 × 768, iPad"),
    ("1280x800", "1280 × 800"),
    ("1920x1080", "1920 × 1080"),
];

/// Where the tablet's screen stands and how big it is, kept in
/// `tablet.env` next to the config, which the VNC unit reads too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub side: String,
    pub alignment: String,
    pub size: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            side: "right".into(),
            alignment: "center".into(),
            size: SIZES[0].0.into(),
        }
    }
}

impl Settings {
    /// Read `KEY=value` lines; anything missing or unknown keeps its default.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "GLIFF_TABLET_POSITION" if SIDES.iter().any(|(s, ..)| *s == value) => {
                    settings.side = value.into()
                }
                "GLIFF_TABLET_ALIGN" if ALIGNMENTS.iter().any(|(a, _)| *a == value) => {
                    settings.alignment = value.into()
                }
                "GLIFF_TABLET_SIZE" if valid_size(value) => settings.size = value.into(),
                _ => {}
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        format!(
            "# Written by gliff's tablet dialog; read by {VNC_UNIT}.\n\
             GLIFF_TABLET_POSITION={}\nGLIFF_TABLET_ALIGN={}\nGLIFF_TABLET_SIZE={}\n",
            self.side, self.alignment, self.size
        )
    }

    fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("env.tmp");
        std::fs::write(&tmp, self.render())?;
        std::fs::rename(&tmp, path)
    }
}

/// `WIDTHxHEIGHT` with both even and within what a screen can be.
fn valid_size(size: &str) -> bool {
    let Some((w, h)) = size.split_once('x') else {
        return false;
    };
    let ok = |v: &str| {
        v.parse::<u32>()
            .is_ok_and(|n| (320..=8192).contains(&n) && n % 2 == 0)
    };
    ok(w) && ok(h)
}

/// The VNC password: the first line of `vnc-password`, made on first use.
/// VNC authentication reads only its first 8 characters.
fn password(dir: &Path) -> std::io::Result<String> {
    let path = dir.join("vnc-password");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let line: String = text.lines().next().unwrap_or("").chars().take(8).collect();
        if !line.is_empty() {
            return Ok(line);
        }
    }
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut random = [0u8; 8];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let made: String = random
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    writeln!(file, "{made}")?;
    Ok(made)
}

/// This computer's address on the network a tablet would use: the source of
/// the default route. Connecting a UDP socket sends nothing.
fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

/// The page's address, password included so the icon an iPad's "Add to Home
/// Screen" makes opens without asking for it.
pub fn page_url(address: IpAddr, password: &str) -> String {
    let mut encoded = String::new();
    for b in password.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("http://{address}:{WEB_PORT}/#pw={encoded}")
}

/// Run `systemctl --user ARGS`: whether it succeeded, and what it said.
async fn systemctl(args: &[&str]) -> (bool, String) {
    let mut argv: Vec<&std::ffi::OsStr> = vec!["systemctl".as_ref(), "--user".as_ref()];
    argv.extend(args.iter().map(|a| std::ffi::OsStr::new(*a)));
    let process = match gio::Subprocess::newv(
        &argv,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_MERGE,
    ) {
        Ok(p) => p,
        Err(e) => return (false, e.to_string()),
    };
    match process.communicate_utf8_future(None).await {
        Ok((out, _)) => (
            process.is_successful(),
            out.map(|s| s.trim().to_owned()).unwrap_or_default(),
        ),
        Err(e) => (false, e.to_string()),
    }
}

async fn running() -> bool {
    systemctl(&["is-active", VNC_UNIT]).await.1 == "active"
}

pub struct Tablet {
    button: gtk::Button,
    window: glib::WeakRef<gtk::Widget>,
    dir: PathBuf,
}

impl Tablet {
    pub fn install(window: &impl IsA<gtk::Widget>, button: &gtk::Button, dir: PathBuf) -> Rc<Self> {
        let this = Rc::new(Self {
            button: button.clone(),
            window: window.as_ref().downgrade(),
            dir,
        });
        let t = this.clone();
        button.connect_clicked(move |_| {
            let t = t.clone();
            glib::spawn_future_local(async move { t.open().await });
        });
        let t = this.clone();
        glib::spawn_future_local(async move { t.show_state(running().await) });
        this
    }

    fn env_path(&self) -> PathBuf {
        self.dir.join("tablet.env")
    }

    fn show_state(&self, on: bool) {
        self.button.set_tooltip_text(Some(if on {
            "A tablet is one more screen; click to change or stop it"
        } else {
            "Use a tablet as a screen"
        }));
        if on {
            self.button.add_css_class("accent");
        } else {
            self.button.remove_css_class("accent");
        }
    }

    async fn open(self: Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let on = running().await;
        self.show_state(on);
        let saved = Settings::load(&self.env_path());
        let url = match password(&self.dir) {
            Ok(pw) => lan_address().map(|a| page_url(a, &pw)),
            Err(e) => {
                tracing::warn!(error = %e, "cannot read or make the VNC password");
                None
            }
        };

        let dialog = adw::AlertDialog::new(
            Some("Use a tablet as a screen"),
            Some(
                "Open this address in the tablet's web browser. On an iPad, add it to \
                 the Home Screen and open it from there to see it full screen. The \
                 address holds the password: share it only with your own devices.",
            ),
        );
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();

        let address = gtk::Box::builder().css_classes(["linked"]).build();
        let entry = gtk::Entry::builder()
            .editable(false)
            .hexpand(true)
            .text(url.as_deref().unwrap_or("No network address found"))
            .build();
        let copy = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text("Copy the address")
            .sensitive(url.is_some())
            .build();
        if let Some(url) = url.clone() {
            copy.connect_clicked(move |b| {
                b.clipboard().set_text(&url);
                b.set_icon_name("object-select-symbolic");
                b.set_tooltip_text(Some("Copied"));
            });
        }
        address.append(&entry);
        address.append(&copy);
        content.append(&address);

        let (grid, sides) = side_picker(&saved.side);
        content.append(&grid);
        let align = alignment_picker(&saved.alignment);
        content.append(&align);
        let labels: Vec<&str> = SIZES.iter().map(|(_, l)| *l).collect();
        let size = gtk::DropDown::from_strings(&labels);
        size.set_selected(
            SIZES
                .iter()
                .position(|(s, _)| *s == saved.size)
                .unwrap_or(0) as u32,
        );
        content.append(&size);
        dialog.set_extra_child(Some(&content));

        dialog.add_response("close", "Close");
        if on {
            dialog.add_response("stop", "Stop Screen");
            dialog.set_response_appearance("stop", adw::ResponseAppearance::Destructive);
            dialog.add_response("start", "Apply");
        } else {
            dialog.add_response("start", "Start Screen");
        }
        dialog.set_response_appearance("start", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("start"));
        dialog.set_close_response("close");

        let response = dialog.choose_future(Some(&window)).await;
        let settings = Settings {
            side: picked_side(&sides).into(),
            alignment: picked_alignment(&align).into(),
            size: SIZES[(size.selected() as usize).min(SIZES.len() - 1)]
                .0
                .into(),
        };
        let failure = match response.as_str() {
            "start" if on && settings == saved => None,
            "start" => match settings.save(&self.env_path()) {
                Err(e) => Some(format!("Cannot save the tablet settings: {e}")),
                // The page stays up while the screen is off, so the tablet
                // can put the screen back by itself.
                Ok(()) => match systemctl(&["start", WEB_UNIT]).await {
                    (false, why) => Some(why),
                    (true, _) => match systemctl(&["restart", VNC_UNIT]).await {
                        (false, why) => Some(why),
                        (true, _) => None,
                    },
                },
            },
            "stop" => match systemctl(&["stop", VNC_UNIT]).await {
                (false, why) => Some(why),
                (true, _) => None,
            },
            _ => None,
        };
        self.show_state(running().await);
        if let Some(why) = failure {
            let error = adw::AlertDialog::new(Some("The tablet screen did not change"), Some(&why));
            error.add_response("close", "Close");
            error.choose_future(Some(&window)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_env_file() {
        let s = Settings {
            side: "below".into(),
            alignment: "end".into(),
            size: "1280x800".into(),
        };
        assert_eq!(Settings::parse(&s.render()), s);
    }

    #[test]
    fn unknown_or_missing_values_keep_the_defaults() {
        let s = Settings::parse("GLIFF_TABLET_POSITION=sideways\nGLIFF_TABLET_SIZE=13x7\njunk\n");
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn sizes_must_be_even_and_plausible() {
        assert!(valid_size("1024x720"));
        assert!(!valid_size("1023x720"));
        assert!(!valid_size("1024"));
        assert!(!valid_size("100x100"));
        assert!(SIZES.iter().all(|(s, _)| valid_size(s)));
    }

    #[test]
    fn the_password_is_escaped_in_the_address() {
        let ip: IpAddr = "192.168.1.31".parse().unwrap();
        assert_eq!(
            page_url(ip, "ab#c&1"),
            "http://192.168.1.31:6080/#pw=ab%23c%261"
        );
    }

    #[test]
    fn a_password_is_made_once_and_kept_private() {
        let dir = tempfile::tempdir().unwrap();
        let first = password(dir.path()).unwrap();
        assert_eq!(first.len(), 8);
        assert_eq!(password(dir.path()).unwrap(), first);
        let mode = std::fs::metadata(dir.path().join("vnc-password"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
    }
}
