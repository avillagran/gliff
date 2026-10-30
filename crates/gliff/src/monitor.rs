//! The header's screen buttons: use the selected machine as one more screen
//! of this one ([`display`](crate::display)), placed where the user said it
//! stands, or drive it with this computer's pointer and keyboard across that
//! edge ([`control`](crate::control)), and stop either again.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use serde::{Deserialize, Serialize};

use crate::control::{self, Report};
use crate::display::{Display, Target};
use crate::{App, Config};

/// Where a machine's screen stands next to this computer's, per machine in
/// the config.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `left`, `right`, `above` or `below` this computer's screens.
    pub side: String,
    /// `start`, `center` or `end`: which edges line up along that side.
    pub alignment: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            side: "right".into(),
            alignment: "start".into(),
        }
    }
}

const SIDES: [(&str, &str, i32, i32); 4] = [
    ("above", "Above", 1, 0),
    ("left", "Left", 0, 1),
    ("right", "Right", 2, 1),
    ("below", "Below", 1, 2),
];
const ALIGNMENTS: [(&str, &str); 3] = [
    ("start", "Align top or left edges"),
    ("center", "Center"),
    ("end", "Align bottom or right edges"),
];

impl Settings {
    fn valid(&self) -> bool {
        SIDES.iter().any(|(s, ..)| *s == self.side)
            && ALIGNMENTS.iter().any(|(a, _)| *a == self.alignment)
    }
}

type Running = Rc<RefCell<Option<Display>>>;

/// What the settings dialog leads to once saved.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Then {
    Nothing,
    UseScreen,
    Control,
}

/// A machine driven with this computer's pointer and keyboard.
struct Driven {
    /// Tells this run's reports from those of an earlier one.
    id: u64,
    side: String,
    handle: control::Handle,
    connected: bool,
}

pub struct Controller {
    ui: Weak<App>,
    button: gtk::Button,
    settings_button: gtk::Button,
    /// The machine whose screen is in use, and its display.
    running: RefCell<Option<(String, Running)>>,
    /// The machine whose screen was used last. Its tab stays the selected
    /// one while no other machine is shown, so the button and the settings
    /// still apply to it after the screen stops.
    chosen: RefCell<Option<String>>,
    message: RefCell<String>,
    control_button: gtk::Button,
    /// The machines driven from this computer's edges, by name.
    driven: RefCell<BTreeMap<String, Driven>>,
    next_id: Cell<u64>,
}

impl Controller {
    pub fn install(
        ui: &Rc<App>,
        button: &gtk::Button,
        settings: &gtk::Button,
        control_button: &gtk::Button,
    ) -> Rc<Self> {
        let this = Rc::new(Self {
            ui: Rc::downgrade(ui),
            button: button.clone(),
            settings_button: settings.clone(),
            running: RefCell::new(None),
            chosen: RefCell::new(None),
            message: RefCell::new(String::new()),
            control_button: control_button.clone(),
            driven: RefCell::new(BTreeMap::new()),
            next_id: Cell::new(0),
        });
        let c = this.clone();
        button.connect_clicked(move |_| {
            if c.running.borrow().is_some() {
                c.stop();
                return;
            }
            let Some(name) = c.selected() else {
                c.note("Select a machine's tab first.");
                return;
            };
            let saved = c.saved(&name);
            match saved.filter(Settings::valid) {
                Some(settings) => c.start(name, &settings),
                None => c.configure(name, Then::UseScreen),
            }
        });
        let c = this.clone();
        settings.connect_clicked(move |_| match c.selected() {
            Some(name) => c.configure(name, Then::Nothing),
            None => c.note("Select a machine's tab first."),
        });
        let c = this.clone();
        control_button.connect_clicked(move |_| {
            let Some(name) = c.selected() else {
                c.note("Select a machine's tab first.");
                return;
            };
            if c.drives(&name) {
                c.stop_control(&name);
                return;
            }
            match c.saved(&name).filter(Settings::valid) {
                Some(settings) => c.start_control(name, &settings),
                None => c.configure(name, Then::Control),
            }
        });
        // The screen would outlive the window with nothing left to stop it:
        // give it back when the window closes. The server removes its output
        // once ssh is gone, even after this process exits.
        let weak = Rc::downgrade(&this);
        ui.window.connect_close_request(move |_| {
            if let Some(c) = weak.upgrade() {
                c.stop();
                // Joined here, so each pointer is back before the app goes.
                c.driven.borrow_mut().clear();
            }
            glib::Propagation::Proceed
        });
        this.refresh();
        this
    }

    /// The machine of the selected tab, when it is reached over ssh.
    fn selected(&self) -> Option<String> {
        let ui = self.ui.upgrade()?;
        let active = ui.active.borrow();
        match active.as_ref() {
            Some(s) => matches!(s.endpoint, crate::Endpoint::Ssh(_)).then(|| s.name.clone()),
            None => self.chosen(),
        }
    }

    /// The machine whose tab is selected without its session shown.
    pub fn chosen(&self) -> Option<String> {
        let ui = self.ui.upgrade()?;
        let name = self.chosen.borrow().clone()?;
        crate::tab_names(&ui).contains(&name).then_some(name)
    }

    /// `name`'s tab is selected without its session shown.
    pub fn choose(&self, name: &str) {
        *self.chosen.borrow_mut() = Some(name.to_owned());
    }

    /// Another machine is shown: its tab is the selected one now.
    pub fn unchoose(&self) {
        self.chosen.borrow_mut().take();
    }

    fn saved(&self, name: &str) -> Option<Settings> {
        let ui = self.ui.upgrade()?;
        Config::load(&ui.config_path).monitors.get(name).cloned()
    }

    /// Whether `name`'s screen is in use.
    pub fn owns(&self, name: &str) -> bool {
        self.running
            .borrow()
            .as_ref()
            .is_some_and(|(n, _)| n == name)
    }

    fn note(&self, message: &str) {
        *self.message.borrow_mut() = message.into();
        if let Some(ui) = self.ui.upgrade() {
            crate::show_status(&ui, message);
        }
    }

    /// Whether `name` is driven from this computer's edge.
    pub fn drives(&self, name: &str) -> bool {
        self.driven.borrow().contains_key(name)
    }

    /// Whether a driven `name` answers.
    pub fn drives_connected(&self, name: &str) -> bool {
        self.driven.borrow().get(name).is_some_and(|d| d.connected)
    }

    /// What the window says while `name`'s tab is selected and it is driven.
    pub fn control_status(&self, name: &str) -> String {
        match self.driven.borrow().get(name) {
            Some(d) if d.connected => format!(
                "Push the pointer past the {} edge to control {name}. Cross back, or \
                 press the release hotkey, to return.",
                d.side
            ),
            Some(_) => format!("Connecting to {name}…"),
            None => String::new(),
        }
    }

    /// Set the control button for the selected machine.
    pub fn update_control_button(&self) {
        let selected = self.selected();
        let driving = selected.as_deref().is_some_and(|n| self.drives(n));
        self.control_button.set_icon_name(if driving {
            crate::STOP_ICON
        } else {
            "input-mouse-symbolic"
        });
        self.control_button
            .set_tooltip_text(Some(&match (&selected, driving) {
                (Some(name), true) => format!("Stop controlling {name}"),
                (Some(name), false) => {
                    format!("Control {name} with this mouse and keyboard, across a screen edge")
                }
                (None, _) => "Control the selected machine with this mouse and keyboard".into(),
            }));
    }

    fn refresh(&self) {
        let running = self.running.borrow().is_some();
        self.button.set_icon_name(if running {
            crate::STOP_ICON
        } else {
            "video-display-symbolic"
        });
        self.button.set_tooltip_text(Some(if running {
            "Stop using the other machine's screen"
        } else {
            "Use the selected machine's screen as one more screen"
        }));
        self.settings_button.set_sensitive(!running);
        if let Some(ui) = self.ui.upgrade() {
            crate::refresh_tabs(&ui);
        }
        self.update_control_button();
    }

    /// Ask where `name`'s screen stands, then do what `then` says.
    fn configure(self: &Rc<Self>, name: String, then: Then) {
        if self.running.borrow().is_some() && then != Then::Control {
            return;
        }
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let saved = self.saved(&name).unwrap_or_default();
        let (title, body) = match then {
            Then::Control => (
                "Control with this mouse",
                format!(
                    "Where is {name}'s screen relative to this computer's? Push the \
                     pointer past that edge to move onto {name}; your keyboard follows it."
                ),
            ),
            _ => (
                "Use as a screen",
                format!(
                    "Where is {name}'s screen relative to this computer's? Your apps keep \
                     running here; {name} shows them and its keyboard and mouse control them."
                ),
            ),
        };
        let dialog = adw::AlertDialog::new(Some(title), Some(&body));
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();
        let grid = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(6)
            .halign(gtk::Align::Center)
            .build();
        grid.attach(&gtk::Label::new(Some("This computer")), 1, 1, 1, 1);
        let mut sides: Vec<(&'static str, gtk::ToggleButton)> = Vec::new();
        for (side, label, x, y) in SIDES {
            let b = gtk::ToggleButton::with_label(label);
            if let Some((_, first)) = sides.first() {
                b.set_group(Some(first));
            }
            b.set_active(saved.side == side);
            grid.attach(&b, x, y, 1, 1);
            sides.push((side, b));
        }
        content.append(&grid);
        let labels: Vec<&str> = ALIGNMENTS.iter().map(|(_, l)| *l).collect();
        let align = gtk::DropDown::from_strings(&labels);
        align.set_selected(
            ALIGNMENTS
                .iter()
                .position(|(a, _)| *a == saved.alignment)
                .unwrap_or(0) as u32,
        );
        // Only a screen in use lines up along the side; the pointer crosses
        // anywhere along it.
        align.set_visible(then != Then::Control);
        content.append(&align);
        dialog.set_extra_child(Some(&content));
        dialog.add_responses(&[
            ("cancel", "Cancel"),
            (
                "save",
                match then {
                    Then::Nothing => "Save",
                    Then::UseScreen => "Save and Use",
                    Then::Control => "Save and Control",
                },
            ),
        ]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        let c = self.clone();
        glib::spawn_future_local(async move {
            if dialog.choose_future(Some(&ui.window)).await != "save" {
                return;
            }
            let settings = Settings {
                side: sides
                    .iter()
                    .find(|(_, b)| b.is_active())
                    .map_or("right", |(s, _)| *s)
                    .into(),
                alignment: ALIGNMENTS[(align.selected() as usize).min(2)].0.into(),
            };
            let mut config = Config::load(&ui.config_path);
            config.monitors.insert(name.clone(), settings.clone());
            if let Err(e) = config.save(&ui.config_path) {
                c.note(&format!("Cannot save the screen settings: {e}"));
                return;
            }
            match then {
                Then::Nothing => {}
                Then::UseScreen => c.start(name, &settings),
                Then::Control => c.start_control(name, &settings),
            }
        });
    }

    fn start(self: &Rc<Self>, name: String, settings: &Settings) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        if self.running.borrow().is_some() {
            return;
        }
        if self.drives(&name) {
            self.note(&format!("Stop controlling {name} first."));
            return;
        }
        // Showing that machine here while it shows this screen would put
        // the screen inside itself. Its tab stays selected, not the next one.
        crate::show_lending(&ui, &name);
        let target = Target {
            host: name.clone(),
            remote_bin: ui.cli.remote_gliff.clone(),
        };
        let weak = Rc::downgrade(self);
        let ended_name = name.clone();
        let started = Display::start(&target, &settings.side, &settings.alignment, move |why| {
            let Some(c) = weak.upgrade() else {
                return;
            };
            if c.owns(&ended_name) {
                c.running.borrow_mut().take();
            }
            c.note(&match why {
                Some(why) => format!("{ended_name}'s screen stopped: {why}"),
                None => format!("Stopped using {ended_name}'s screen."),
            });
            c.refresh();
        });
        match started {
            Ok(display) => {
                *self.running.borrow_mut() = Some((name.clone(), display));
                self.note(&format!("Using {name}'s screen. Stop gives it back."));
            }
            Err(e) => self.note(&format!("Cannot use {name}'s screen: {e}")),
        }
        self.refresh();
    }

    /// Drive `name` from the edge on `settings.side`, until stopped.
    fn start_control(self: &Rc<Self>, name: String, settings: &Settings) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        if self.owns(&name) {
            self.note(&format!(
                "{name}'s screen is in use as a screen of this computer. Stop that first."
            ));
            return;
        }
        let taken = self
            .driven
            .borrow()
            .iter()
            .find(|(_, d)| d.side == settings.side)
            .map(|(n, _)| n.clone());
        if let Some(other) = taken {
            self.note(&format!(
                "{other} is already on the {} side. Stop controlling it, or place {name} \
                 on another side.",
                settings.side
            ));
            return;
        }
        let Some(side) = hypr_edge::Side::parse(&settings.side) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let handle = control::Handle::spawn(
            vec![control::Control {
                side,
                host: name.clone(),
            }],
            ui.cli.server_bin.clone(),
            ui.cli.release_hotkey.clone(),
            tx,
        );
        let handle = match handle {
            Ok(handle) => handle,
            Err(e) => {
                self.note(&format!("Cannot control {name}: {e}"));
                return;
            }
        };
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        self.driven.borrow_mut().insert(
            name.clone(),
            Driven {
                id,
                side: settings.side.clone(),
                handle,
                connected: false,
            },
        );
        // Showing that machine here as well would only take the pointer
        // away from its edge.
        crate::show_controlling(&ui, &name);
        self.refresh();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let Some(c) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            while let Ok(report) = rx.try_recv() {
                if !c.on_report(&name, id, report) {
                    return glib::ControlFlow::Break;
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// Act on a report from `name`'s control run `id`; false once that run
    /// is over.
    fn on_report(&self, name: &str, id: u64, report: Report) -> bool {
        let current = self.driven.borrow().get(name).is_some_and(|d| d.id == id);
        if !current {
            return !matches!(report, Report::Ended { .. });
        }
        let shown = self.chosen().as_deref() == Some(name);
        match report {
            Report::Connected { .. } => {
                self.set_connected(name, true);
                if shown {
                    self.note(&self.control_status(name));
                }
            }
            Report::Disconnected { error, .. } => {
                self.set_connected(name, false);
                if shown {
                    self.note(&match error {
                        Some(e) => format!("Cannot reach {name}: {e}. Trying again…"),
                        None => format!("{name} went away. Trying again…"),
                    });
                }
            }
            Report::Ended { error } => {
                self.driven.borrow_mut().remove(name);
                self.note(&match error {
                    Some(e) => format!("Stopped controlling {name}: {e}"),
                    None => format!("Stopped controlling {name}."),
                });
                self.refresh();
                return false;
            }
        }
        true
    }

    fn set_connected(&self, name: &str, connected: bool) {
        if let Some(d) = self.driven.borrow_mut().get_mut(name) {
            d.connected = connected;
        }
        if let Some(ui) = self.ui.upgrade() {
            crate::refresh_tabs(&ui);
        }
    }

    /// Stop driving `name`. Its thread gives the pointer back and is joined
    /// off the main loop.
    pub fn stop_control(&self, name: &str) {
        let Some(mut driven) = self.driven.borrow_mut().remove(name) else {
            return;
        };
        driven.handle.stop();
        std::thread::spawn(move || drop(driven));
        self.note(&format!("Stopped controlling {name}."));
        self.refresh();
    }

    pub fn stop(&self) {
        if let Some((_, display)) = self.running.borrow().as_ref() {
            if let Some(d) = display.borrow_mut().as_mut() {
                d.stop();
            }
        }
    }
}
