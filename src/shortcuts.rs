//! Remote stop via GApplication action.
//!
//! Registers a `stop-recording` action on the application that can be
//! triggered remotely via D-Bus. Running `plick --stop` from another
//! process activates this action on the already-running instance.
//!
//! To bind a global keyboard shortcut, add a GNOME custom shortcut:
//!   Name: Stop Plick Recording
//!   Command: plick --stop

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

/// Register the `stop-recording` action on the application.
/// When activated, it sends `()` on `stop_tx`.
pub fn register_stop_action(app: &libadwaita::Application, stop_tx: std::sync::mpsc::Sender<()>) {
    let action = gio::SimpleAction::new("stop-recording", None);
    action.connect_activate(move |_, _| {
        eprintln!("stop-recording action activated");
        let _ = stop_tx.send(());
    });
    app.add_action(&action);
}

/// Register the `--stop` command-line option on the application.
pub fn register_stop_option(app: &libadwaita::Application) {
    app.add_main_option(
        "stop",
        glib::Char::from(0),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Stop an in-progress recording",
        None,
    );
}

/// Check if `--stop` was passed in the command line, and if so,
/// activate the stop action. Returns `true` if `--stop` was handled.
pub fn handle_command_line(app: &libadwaita::Application, cmdline: &gio::ApplicationCommandLine) -> bool {
    let options = cmdline.options_dict();
    if options.contains("stop") {
        app.activate_action("stop-recording", None);
        true
    } else {
        false
    }
}
