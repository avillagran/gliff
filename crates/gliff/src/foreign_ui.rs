//! The window's side of RDP and RustDesk machines: the login, from the
//! keyring or asked for; and, when a machine added by name does not answer
//! over ssh, a look for RDP or RustDesk on it and the offer to use that.

use adw::prelude::*;
use gliff_foreign::{Credentials, Kind, Target};
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;

use crate::secrets::{self, Saved};

/// The login for `name`: the one saved in the keyring, unless `refused`
/// says it was just turned down; otherwise asked for, and saved when the
/// user says so. `None` when the user cancelled.
pub async fn credentials(
    window: &impl IsA<gtk::Widget>,
    name: &str,
    target: &Target,
    refused: Option<&str>,
) -> Option<Credentials> {
    let saved = {
        let name = name.to_string();
        gio::spawn_blocking(move || secrets::load(&name))
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("the keyring lookup failed")))
    };
    let saved = match saved {
        Ok(saved) => saved,
        Err(e) => {
            tracing::warn!(machine = %name, error = %format!("{e:#}"), "keyring");
            None
        }
    };
    if refused.is_none() {
        if let Some(s) = &saved {
            return Some(Credentials {
                user: s.user.clone(),
                password: s.password.clone(),
            });
        }
    }
    let user = saved
        .as_ref()
        .map(|s| s.user.clone())
        .filter(|u| !u.is_empty())
        .or_else(|| target.user.clone())
        .unwrap_or_default();
    let (credentials, remember) = ask(window, name, target, &user, refused).await?;
    if remember {
        let name = name.to_string();
        let saved = Saved {
            user: credentials.user.clone(),
            password: credentials.password.clone(),
        };
        let stored = gio::spawn_blocking(move || secrets::store(&name, &saved)).await;
        match stored {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %format!("{e:#}"), "cannot save the password"),
            Err(_) => tracing::warn!("cannot save the password"),
        }
    }
    Some(credentials)
}

/// Forget the password saved for `name`, as when the machine is forgotten.
pub fn forget(name: &str) {
    let name = name.to_string();
    glib::spawn_future_local(async move {
        let n = name.clone();
        if let Ok(Err(e)) = gio::spawn_blocking(move || secrets::delete(&n)).await {
            tracing::warn!(machine = %name, error = %format!("{e:#}"), "cannot forget the password");
        }
    });
}

/// The dialog: the user (for RDP), the password, and whether to keep them.
async fn ask(
    window: &impl IsA<gtk::Widget>,
    name: &str,
    target: &Target,
    user: &str,
    refused: Option<&str>,
) -> Option<(Credentials, bool)> {
    let kind = target.kind;
    let body = match refused {
        Some(why) => format!("{why}. Try again."),
        None => match kind {
            Kind::Rdp => format!("Log in to {name} through Remote Desktop."),
            Kind::RustDesk => format!(
                "The permanent password set in {name}'s RustDesk. Its \"Enable direct IP \
                 access\" option must be on."
            ),
        },
    };
    let dialog = adw::AlertDialog::new(Some(&format!("{} login", kind.label())), Some(&body));
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let user_row = adw::EntryRow::builder().title("User").text(user).build();
    if kind.takes_user() {
        list.append(&user_row);
    }
    let password_row = adw::PasswordEntryRow::builder().title("Password").build();
    list.append(&password_row);
    let remember = adw::SwitchRow::builder()
        .title("Remember in the keyring")
        .active(true)
        .build();
    list.append(&remember);
    dialog.set_extra_child(Some(&list));
    dialog.add_responses(&[("cancel", "Cancel"), ("connect", "Connect")]);
    dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("connect"));
    dialog.set_close_response("cancel");
    {
        let dialog = dialog.clone();
        password_row.connect_entry_activated(move |_| {
            dialog.emit_by_name::<()>("response", &[&"connect"]);
        });
    }
    if kind.takes_user() && user.is_empty() {
        user_row.grab_focus();
    } else {
        password_row.grab_focus();
    }
    if dialog.choose_future(Some(window)).await != "connect" {
        return None;
    }
    let credentials = Credentials {
        user: if kind.takes_user() {
            user_row.text().trim().to_string()
        } else {
            String::new()
        },
        password: password_row.text().to_string(),
    };
    Some((credentials, remember.is_active()))
}

/// What answers on `host` (an ssh host, maybe `user@host`) for RDP or
/// RustDesk, looked for off the GTK thread.
pub async fn probe(ssh_host: &str) -> Option<Kind> {
    let host = ssh_host
        .rsplit_once('@')
        .map_or(ssh_host, |(_, h)| h)
        .to_string();
    gio::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        rt.block_on(gliff_foreign::probe(&host))
    })
    .await
    .ok()
    .flatten()
}

/// Ask whether to reach `name`, which did not answer over ssh, through
/// `kind` instead.
pub async fn confirm_switch(window: &impl IsA<gtk::Widget>, name: &str, kind: Kind) -> bool {
    let body = match kind {
        Kind::Rdp => format!(
            "{name} does not answer over ssh, but it runs a Remote Desktop server. gliff \
             can show it and control it through RDP instead, with its user and password. \
             On Windows, Remote Desktop opens a session of its own and locks the screen \
             at the machine itself."
        ),
        Kind::RustDesk => format!(
            "{name} does not answer over ssh, but it runs RustDesk with direct IP access. \
             gliff can show it and control it through RustDesk instead, with its \
             permanent password."
        ),
    };
    let dialog = adw::AlertDialog::new(Some(&format!("Use {}?", kind.label())), Some(&body));
    dialog.add_responses(&[("cancel", "Keep Trying ssh"), ("switch", "Use It")]);
    dialog.set_response_appearance("switch", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("switch"));
    dialog.set_close_response("cancel");
    dialog.choose_future(Some(window)).await == "switch"
}

/// Whether an ssh failure means the machine did not let ssh in at all, so
/// another protocol is worth a look; not a failure after login, such as a
/// missing gliff-server.
pub fn ssh_unreachable(error: &str) -> bool {
    let e = error.to_ascii_lowercase();
    [
        "connection refused",
        "connection timed out",
        "operation timed out",
        "no route to host",
        "connection closed by",
        "connection reset",
        "permission denied",
        "host is down",
        "network is unreachable",
    ]
    .iter()
    .any(|s| e.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_an_unreachable_ssh_from_other_failures() {
        assert!(ssh_unreachable(
            "ssh: connect to host pc port 22: Connection refused"
        ));
        assert!(ssh_unreachable(
            "ssh: connect to host pc port 22: Connection timed out"
        ));
        assert!(ssh_unreachable("bob@pc: Permission denied (publickey)."));
        assert!(!ssh_unreachable("the server closed the stream"));
        assert!(!ssh_unreachable("ssh: Could not resolve hostname pc"));
    }
}
