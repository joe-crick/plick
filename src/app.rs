//! Simple toolbar UI for screen recording.
//!
//! A small window with Record/Stop buttons and a folder chooser.
//! The portal handles screen/window selection. Recording captures
//! whatever the user chose in the portal dialog.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, CssProvider,
    FileChooserAction, FileChooserDialog, Label, Orientation,
    ResponseType, Spinner, Window,
};

use crate::config::Config;
use crate::converter;
use crate::recorder::{self, Recorder};
use crate::shortcuts;
use crate::tray;

const APP_ID: &str = "com.github.plick";

pub fn run() -> i32 {
    let app = libadwaita::Application::builder()
        .application_id(APP_ID)
        .flags(gtk4::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // Register --stop CLI option (forwarded to running instance via D-Bus)
    shortcuts::register_stop_option(&app);

    // Channel for remote stop signals (--stop CLI, tray icon)
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let stop_rx = Arc::new(Mutex::new(Some(stop_rx)));

    // Register the stop-recording GApplication action
    shortcuts::register_stop_action(&app, stop_tx.clone());

    // Store stop_tx and stop_rx so build_ui can access them
    let stop_tx_for_ui = stop_tx.clone();
    let stop_rx_for_ui = stop_rx.clone();

    app.connect_activate(move |app| {
        build_ui(app, stop_tx_for_ui.clone(), stop_rx_for_ui.clone());
    });

    // Handle command-line: check for --stop, otherwise activate normally
    app.connect_command_line(|app, cmdline| {
        if !shortcuts::handle_command_line(app, cmdline) {
            app.activate();
        }
        0
    });

    app.run().into()
}

fn load_css() {
    let css = r#"
        button.suggested-action {
            border-radius: 5px;
        }
        .plick-toolbar {
            padding: 6px 12px;
        }
        .plick-rec-dot {
            color: #e01b24;
            font-size: 16px;
        }
    "#;
    let provider = CssProvider::new();
    provider.load_from_data(css);
    gtk4::style_context_add_provider_for_display(
        &gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );
}

fn build_ui(
    app: &libadwaita::Application,
    stop_tx: mpsc::Sender<()>,
    stop_rx: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
) {
    load_css();

    let config = Config::load();

    // Startup checks
    let missing: Vec<&str> = [
        (!recorder::is_ffmpeg_available(), "ffmpeg"),
        (!recorder::is_gstreamer_available(), "gst-launch-1.0"),
    ]
    .iter()
    .filter(|(m, _)| *m)
    .map(|(_, name)| *name)
    .collect();

    if !missing.is_empty() {
        let dialog = gtk4::MessageDialog::builder()
            .message_type(gtk4::MessageType::Error)
            .buttons(gtk4::ButtonsType::Ok)
            .text("Missing dependencies")
            .secondary_text(&format!(
                "Plick requires: {}\n\nInstall with:\nsudo dnf install ffmpeg gstreamer1-plugins-base gstreamer1-plugins-good",
                missing.join(", ")
            ))
            .build();
        dialog.connect_response(|d, _| {
            d.close();
            std::process::exit(1);
        });
        dialog.present();
        return;
    }

    let recorder = Rc::new(RefCell::new(Recorder::new(config.clone())));
    let config = Rc::new(RefCell::new(config));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Plick")
        .default_width(340)
        .default_height(48)
        .resizable(false)
        .build();

    // --- Toolbar widgets ---
    let record_btn = Button::builder()
        .label("Record")
        .css_classes(["suggested-action"])
        .build();

    let stop_btn = Button::builder()
        .label("Stop")
        .css_classes(["destructive-action"])
        .visible(false)
        .build();

    let timer_label = Label::builder()
        .label("")
        .css_classes(["monospace"])
        .visible(false)
        .build();

    let status_label = Label::builder()
        .label("Ready")
        .hexpand(true)
        .halign(Align::End)
        .build();

    // Folder button
    let folder_btn = Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text(&format!("Save to: {}", config.borrow().output_dir.display()))
        .build();

    {
        let config = config.clone();
        let window_ref = window.clone();
        let folder_btn_ref = folder_btn.clone();

        folder_btn.connect_clicked(move |_| {
            let chooser = FileChooserDialog::new(
                Some("Choose output folder"),
                Some(&window_ref),
                FileChooserAction::SelectFolder,
                &[
                    ("Cancel", ResponseType::Cancel),
                    ("Select", ResponseType::Accept),
                ],
            );
            let current = config.borrow().output_dir.clone();
            let _ = chooser.set_current_folder(Some(&gtk4::gio::File::for_path(&current)));

            let config = config.clone();
            let btn = folder_btn_ref.clone();
            chooser.connect_response(move |ch, resp| {
                if resp == ResponseType::Accept {
                    if let Some(file) = ch.file() {
                        if let Some(path) = file.path() {
                            eprintln!("Output dir changed to: {}", path.display());
                            config.borrow_mut().output_dir = path.clone();
                            let _ = config.borrow().save();
                            btn.set_tooltip_text(Some(&format!("Save to: {}", path.display())));
                        }
                    }
                }
                ch.close();
            });
            chooser.present();
        });
    }

    let toolbar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .css_classes(["plick-toolbar"])
        .build();
    toolbar.append(&record_btn);
    toolbar.append(&stop_btn);
    toolbar.append(&timer_label);
    toolbar.append(&folder_btn);
    toolbar.append(&status_label);

    window.set_child(Some(&toolbar));

    // Tray handle — set when recording, cleared when stopped.
    // Arc<Mutex> because it's shared with the tray-spawning thread.
    let tray_handle: Arc<Mutex<Option<crate::tray::TrayHandle>>> =
        Arc::new(Mutex::new(None));

    // --- Shared stop logic ---
    // Wrapped in Rc so it can be called from stop button, global shortcut, or tray.
    let do_stop: Rc<dyn Fn()> = {
        let recorder = recorder.clone();
        let config = config.clone();
        let record_btn = record_btn.clone();
        let stop_btn = stop_btn.clone();
        let timer_label = timer_label.clone();
        let status_label = status_label.clone();
        let folder_btn = folder_btn.clone();
        let tray_handle = tray_handle.clone();

        Rc::new(move || {
            if !recorder.borrow().is_recording() {
                return;
            }

            let stop_result = recorder.borrow_mut().stop();

            // Remove tray icon
            tray_handle.lock().unwrap().take();

            // Reset toolbar immediately
            stop_btn.set_visible(false);
            timer_label.set_visible(false);
            record_btn.set_visible(true);
            folder_btn.set_visible(true);

            match stop_result {
                Ok(ref temp_video_path) => {
                    let file_size = std::fs::metadata(temp_video_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    eprintln!(
                        "Recording stopped. Temp video: {} ({} bytes)",
                        temp_video_path.display(),
                        file_size
                    );

                    if file_size == 0 {
                        eprintln!("Warning: temp video is empty");
                        status_label.set_label("Recording failed (empty file)");
                        return;
                    }

                    let _ = recorder
                        .borrow_mut()
                        .begin_converting(temp_video_path.clone());

                    status_label.set_label("Converting...");

                    show_converting_then_save(
                        recorder.clone(),
                        config.clone(),
                        status_label.clone(),
                        temp_video_path.clone(),
                    );
                }
                Err(e) => {
                    eprintln!("Failed to stop: {e}");
                    status_label.set_label("Stop failed");
                }
            }
        })
    };

    // --- Poll for remote stop signals (--stop CLI / tray) ---
    {
        let do_stop = do_stop.clone();
        let stop_rx = stop_rx.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            if let Ok(guard) = stop_rx.lock() {
                if let Some(ref rx) = *guard {
                    while rx.try_recv().is_ok() {
                        do_stop();
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    // --- Record button ---
    {
        let recorder = recorder.clone();
        let record_btn_ref = record_btn.clone();
        let stop_btn_ref = stop_btn.clone();
        let timer_label_ref = timer_label.clone();
        let status_label_ref = status_label.clone();
        let folder_btn_ref = folder_btn.clone();
        let tray_handle = tray_handle.clone();
        let stop_tx = stop_tx.clone();

        record_btn.connect_clicked(move |_| {
            let recorder = recorder.clone();
            let record_btn = record_btn_ref.clone();
            let stop_btn = stop_btn_ref.clone();
            let timer_label = timer_label_ref.clone();
            let status_label = status_label_ref.clone();
            let folder_btn = folder_btn_ref.clone();
            let tray_handle = tray_handle.clone();
            let stop_tx = stop_tx.clone();

            record_btn.set_visible(false);
            folder_btn.set_visible(false);
            status_label.set_label("Requesting screen...");

            let (tx, rx) = std::sync::mpsc::channel();

            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let result = rt.block_on(recorder::request_screencast());
                let _ = tx.send(result);
            });

            glib::timeout_add_local(Duration::from_millis(50), move || {
                match rx.try_recv() {
                    Ok(Ok((node_id, _pw_fd))) => {
                        let start_result =
                            recorder.borrow_mut().start_with_stream(node_id, None);

                        match start_result {
                            Ok(()) => {
                                stop_btn.set_visible(true);
                                timer_label.set_visible(true);
                                timer_label.set_label("00:00");
                                status_label.set_label("Recording");

                                // Spawn tray icon — the thread must stay alive
                                // so the tokio runtime keeps processing D-Bus calls.
                                let tray_stop_tx = stop_tx.clone();
                                let tray_handle_ref = tray_handle.clone();
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Runtime::new().unwrap();
                                    rt.block_on(async {
                                        match tray::spawn_tray(tray_stop_tx).await {
                                            Ok(handle) => {
                                                *tray_handle_ref.lock().unwrap() = Some(handle);
                                                // Keep the runtime alive until the connection
                                                // is dropped (when tray_handle is taken/cleared).
                                                std::future::pending::<()>().await;
                                            }
                                            Err(e) => {
                                                eprintln!("Tray icon failed: {e}");
                                            }
                                        }
                                    });
                                });

                                let rec = recorder.clone();
                                let tl = timer_label.clone();
                                glib::timeout_add_local(Duration::from_millis(500), move || {
                                    let r = rec.borrow();
                                    if let Some(elapsed) = r.elapsed() {
                                        let s = elapsed.as_secs();
                                        tl.set_label(&format!("{:02}:{:02}", s / 60, s % 60));
                                        glib::ControlFlow::Continue
                                    } else {
                                        glib::ControlFlow::Break
                                    }
                                });
                            }
                            Err(e) => {
                                eprintln!("Failed to start recording: {e}");
                                record_btn.set_visible(true);
                                folder_btn.set_visible(true);
                                status_label.set_label("Failed to start");
                            }
                        }
                        glib::ControlFlow::Break
                    }
                    Ok(Err(e)) => {
                        eprintln!("Portal error: {e}");
                        recorder.borrow_mut().cancel();
                        record_btn.set_visible(true);
                        folder_btn.set_visible(true);
                        status_label.set_label("Cancelled");
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        glib::ControlFlow::Continue
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        eprintln!("Portal thread died");
                        record_btn.set_visible(true);
                        folder_btn.set_visible(true);
                        status_label.set_label("Error");
                        glib::ControlFlow::Break
                    }
                }
            });
        });
    }

    // --- Stop button ---
    {
        let do_stop = do_stop.clone();
        stop_btn.connect_clicked(move |_| {
            do_stop();
        });
    }

    window.present();
}

fn show_converting_then_save(
    recorder: Rc<RefCell<Recorder>>,
    config: Rc<RefCell<Config>>,
    status_label: Label,
    temp_video_path: PathBuf,
) {
    let conv_window = Window::builder()
        .title("Plick — Converting")
        .default_width(280)
        .default_height(80)
        .build();

    let spinner = Spinner::builder().spinning(true).build();
    let label = Label::new(Some("Converting to GIF..."));

    let vbox = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .halign(Align::Center)
        .build();
    vbox.append(&spinner);
    vbox.append(&label);

    conv_window.set_child(Some(&vbox));
    conv_window.present();

    let gif_fps = config.borrow().gif_fps;
    let gif_width = config.borrow().gif_width;
    let gif_colors = config.borrow().gif_colors;
    let output_dir = config.borrow().output_dir.clone();
    let _ = std::fs::create_dir_all(&output_dir);
    let dest = converter::generate_output_filename(&output_dir);
    eprintln!("Will save GIF to: {}", dest.display());

    let dest_for_thread = dest.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Result<PathBuf, String>>();

    std::thread::spawn(move || {
        eprintln!("Converting {} -> {}", temp_video_path.display(), dest_for_thread.display());
        let result =
            converter::convert(&temp_video_path, &dest_for_thread, gif_fps, gif_width, gif_colors, |_| {});
        match &result {
            Ok(r) => eprintln!("Saved: {} ({} bytes)", r.gif_path.display(), r.gif_size_bytes),
            Err(e) => eprintln!("Conversion failed: {e:#}"),
        }
        let _ = std::fs::remove_file(&temp_video_path);
        let _ = tx.send(result.map(|r| r.gif_path).map_err(|e| e.to_string()));
    });

    let conv_window_ref = conv_window.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        match rx.try_recv() {
            Ok(Ok(gif_path)) => {
                recorder.borrow_mut().cancel();
                conv_window_ref.close();
                let name = gif_path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                status_label.set_label(&format!("Saved: {name}"));
                glib::ControlFlow::Break
            }
            Ok(Err(e)) => {
                eprintln!("Conversion failed: {e}");
                recorder.borrow_mut().cancel();
                conv_window_ref.close();
                status_label.set_label("Conversion failed");
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                recorder.borrow_mut().cancel();
                conv_window_ref.close();
                status_label.set_label("Conversion error");
                glib::ControlFlow::Break
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_is_valid() {
        assert!(!APP_ID.is_empty());
        assert!(APP_ID.contains('.'));
        assert!(APP_ID
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_'));
    }
}
