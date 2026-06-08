//! Accessibility: AT-SPI2 emission for screen readers (Orca) (M5).
//!
//! Promoted to **V1** per the tribunal arbiter's dissent + the Hume audit, which
//! flagged framing the a11y loss as "acceptable" a smuggled normative claim.
//! Override-redirect popups are invisible to AT-SPI by default, so the daemon
//! must self-register and emit announcements for each shown notification.
//!
//! Increment 1 = announcements only: a single root accessible (role APPLICATION)
//! with no child tree, registered on the a11y bus; on each shown popup we emit
//! `org.a11y.atspi.Event.Object.Announcement` so a screen reader speaks it. A
//! fully navigable per-notification tree (Text + Action interfaces) is a later
//! phase. There is no mature Rust *provider* crate (`atspi-rs` is client-side),
//! so the interfaces are hand-rolled with zbus. Off unless `a11y-announce`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tracing::info;
use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::Connection;

/// Our accessible root object path (the at-spi convention for an app root).
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
/// The at-spi "null" reference path (empty child / no-such-object).
const NULL_PATH: &str = "/org/a11y/atspi/null";
const REGISTRY_NAME: &str = "org.a11y.atspi.Registry";
/// AtspiRole::Application (verified against Atspi-2.0.gir).
const ROLE_APPLICATION: u32 = 75;
/// AtspiLive politeness for announcements.
const LIVE_POLITE: i32 = 1;
const LIVE_ASSERTIVE: i32 = 2;

/// An at-spi object reference `(bus_name, object_path)` — the `(so)` type.
type ObjectRef = (String, OwnedObjectPath);

fn obj_ref(name: &str, path: &str) -> ObjectRef {
    (
        name.to_string(),
        OwnedObjectPath::try_from(path).expect("static valid object path"),
    )
}

/// The `Accessible` facet of our root object (zbus needs one struct per served
/// interface; both are served at [`ROOT_PATH`]).
struct AccessibleIface {
    bus_name: String,
    parent: ObjectRef,
}

/// The `Application` facet of our root object.
struct ApplicationIface {
    id: i32,
}

#[zbus::interface(name = "org.a11y.atspi.Accessible")]
impl AccessibleIface {
    #[zbus(property)]
    fn name(&self) -> String {
        "gnome-notistack".into()
    }
    #[zbus(property)]
    fn description(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn parent(&self) -> ObjectRef {
        self.parent.clone()
    }
    #[zbus(property)]
    fn child_count(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn locale(&self) -> String {
        std::env::var("LANG").unwrap_or_default()
    }
    #[zbus(property)]
    fn accessible_id(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn help_text(&self) -> String {
        String::new()
    }

    fn get_child_at_index(&self, _index: i32) -> ObjectRef {
        obj_ref("", NULL_PATH)
    }
    fn get_children(&self) -> Vec<ObjectRef> {
        Vec::new()
    }
    fn get_index_in_parent(&self) -> i32 {
        -1
    }
    fn get_relation_set(&self) -> Vec<(u32, Vec<ObjectRef>)> {
        Vec::new()
    }
    fn get_role(&self) -> u32 {
        ROLE_APPLICATION
    }
    fn get_role_name(&self) -> String {
        "application".into()
    }
    fn get_localized_role_name(&self) -> String {
        "application".into()
    }
    fn get_state(&self) -> Vec<u32> {
        vec![0, 0] // 64-bit state bitset as two u32 words; no states set
    }
    fn get_attributes(&self) -> HashMap<String, String> {
        HashMap::new()
    }
    fn get_application(&self) -> ObjectRef {
        obj_ref(&self.bus_name, ROOT_PATH)
    }
    fn get_interfaces(&self) -> Vec<String> {
        vec![
            "org.a11y.atspi.Accessible".into(),
            "org.a11y.atspi.Application".into(),
        ]
    }
}

#[zbus::interface(name = "org.a11y.atspi.Application")]
impl ApplicationIface {
    #[zbus(property)]
    fn toolkit_name(&self) -> String {
        "gnome-notistack".into()
    }
    #[zbus(property)]
    fn toolkit_version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }
    #[zbus(property)]
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }
    #[zbus(property)]
    fn atspi_version(&self) -> String {
        "2.1".into()
    }
    #[zbus(property)]
    fn interface_version(&self) -> u32 {
        0
    }
    #[zbus(property)]
    fn id(&self) -> i32 {
        self.id
    }
    #[zbus(property)]
    fn set_id(&mut self, id: i32) {
        self.id = id;
    }
    fn get_locale(&self, _lctype: u32) -> String {
        std::env::var("LANG").unwrap_or_default()
    }
    fn get_application_bus_address(&self) -> String {
        String::new()
    }
}

/// Emits AT-SPI announcements; holds the a11y-bus connection alive.
pub struct Announcer {
    conn: Connection,
}

impl Announcer {
    /// Connect to the a11y bus, serve the root accessible, and embed into the
    /// registry. Errors if accessibility isn't running (no a11y bus) — the caller
    /// degrades to no announcements.
    pub async fn register() -> Result<Self> {
        let session = Connection::session().await.context("session bus")?;
        let addr: String =
            zbus::Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus")
                .await?
                .call("GetAddress", &())
                .await
                .context("query a11y bus address")?;
        let conn = zbus::connection::Builder::address(addr.as_str())?
            .build()
            .await
            .context("connect to a11y bus")?;
        let bus_name = conn
            .unique_name()
            .map(|n| n.to_string())
            .unwrap_or_default();

        // Both interfaces live at the same object path (the at-spi root).
        conn.object_server()
            .at(
                ROOT_PATH,
                AccessibleIface {
                    bus_name: bus_name.clone(),
                    // The app root's parent is the desktop/registry root.
                    parent: obj_ref(REGISTRY_NAME, ROOT_PATH),
                },
            )
            .await
            .context("serve Accessible")?;
        conn.object_server()
            .at(ROOT_PATH, ApplicationIface { id: 0 })
            .await
            .context("serve Application")?;

        // Register with the registry: Socket.Embed((our_name, root_path)) -> parent.
        let socket = zbus::Proxy::new(&conn, REGISTRY_NAME, ROOT_PATH, "org.a11y.atspi.Socket")
            .await
            .context("registry Socket proxy")?;
        let _parent: ObjectRef = socket
            .call("Embed", &(obj_ref(&bus_name, ROOT_PATH),))
            .await
            .context("Embed into a11y registry")?;
        info!("registered with the AT-SPI registry (announcements enabled)");
        Ok(Self { conn })
    }

    /// Emit an `object:announcement` so a screen reader speaks `text`.
    pub async fn announce(&self, text: &str, assertive: bool) -> Result<()> {
        let politeness = if assertive {
            LIVE_ASSERTIVE
        } else {
            LIVE_POLITE
        };
        // Event.Object signal: (s detail, i detail1, i detail2, v any_data, a{sv} props).
        let body = (
            "",
            politeness,
            0i32,
            Value::from(text.to_string()),
            HashMap::<String, Value>::new(),
        );
        self.conn
            .emit_signal(
                Option::<&str>::None,
                ROOT_PATH,
                "org.a11y.atspi.Event.Object",
                "Announcement",
                &body,
            )
            .await
            .context("emit announcement")?;
        Ok(())
    }
}
