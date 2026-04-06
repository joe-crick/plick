//! System tray icon via the StatusNotifierItem D-Bus protocol.
//!
//! Shows a recording indicator in the system tray with a "Stop Recording"
//! action. Clicking the tray icon (left or right click) stops the recording.

use std::sync::mpsc::Sender;

use zbus::zvariant;

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";

/// StatusNotifierItem D-Bus interface.
struct StatusNotifierItem {
    stop_tx: Sender<()>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "plick-recorder"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        "Plick — Recording"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        "media-record"
    }

    #[zbus(property)]
    fn menu(&self) -> zvariant::ObjectPath<'_> {
        zvariant::ObjectPath::try_from(MENU_PATH).unwrap()
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    /// Left-click on the tray icon → stop recording.
    fn activate(&self, _x: i32, _y: i32) {
        eprintln!("Tray: icon clicked (Activate)");
        let _ = self.stop_tx.send(());
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        eprintln!("Tray: icon secondary-clicked");
        let _ = self.stop_tx.send(());
    }

    fn scroll(&self, _delta: i32, _orientation: &str) {}
}

/// DBusMenu interface — provides the "Stop Recording" menu item.
struct TrayMenu {
    stop_tx: Sender<()>,
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl TrayMenu {
    fn get_layout(
        &self,
        _parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (
        u32,
        (
            i32,
            std::collections::HashMap<String, zvariant::OwnedValue>,
            Vec<zvariant::OwnedValue>,
        ),
    ) {
        use std::collections::HashMap;

        // Build the "Stop Recording" menu item
        let mut stop_props: HashMap<String, zvariant::OwnedValue> = HashMap::new();
        stop_props.insert(
            "label".to_string(),
            zvariant::Value::from("Stop Recording").try_into().unwrap(),
        );

        let stop_item: zvariant::OwnedValue =
            zvariant::Value::from((1i32, stop_props.clone(), Vec::<zvariant::OwnedValue>::new()))
                .try_into()
                .unwrap();

        // Root item containing the stop item
        let mut root_props: HashMap<String, zvariant::OwnedValue> = HashMap::new();
        root_props.insert(
            "children-display".to_string(),
            zvariant::Value::from("submenu").try_into().unwrap(),
        );

        (1, (0, root_props, vec![stop_item]))
    }

    fn event(&self, id: i32, event_id: &str, _data: zvariant::Value<'_>, _timestamp: u32) {
        eprintln!("Tray menu event: id={id}, event={event_id}");
        if id == 1 && event_id == "clicked" {
            eprintln!("Tray: Stop Recording clicked");
            let _ = self.stop_tx.send(());
        }
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        vec![]
    }
}

/// Handle to a running tray icon. Drop it to remove the tray.
pub struct TrayHandle {
    _conn: zbus::Connection,
}

// Safety: zbus::Connection is Send+Sync internally
unsafe impl Send for TrayHandle {}

/// Spawn a tray icon. Returns a handle; drop it to remove the tray.
pub async fn spawn_tray(
    stop_tx: Sender<()>,
) -> Result<TrayHandle, Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::connection::Builder::session()?
        .name(format!(
            "org.kde.StatusNotifierItem-{}-1",
            std::process::id()
        ))?
        .serve_at(ITEM_PATH, StatusNotifierItem {
            stop_tx: stop_tx.clone(),
        })?
        .serve_at(MENU_PATH, TrayMenu { stop_tx })?
        .build()
        .await?;

    // Register with the StatusNotifierWatcher so the desktop shows our icon
    let watcher: zbus::Proxy<'_> = zbus::Proxy::new(
        &conn,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .await?;

    let service_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let _: () = watcher
        .call("RegisterStatusNotifierItem", &(service_name,))
        .await?;

    eprintln!("Tray icon registered");

    Ok(TrayHandle { _conn: conn })
}
