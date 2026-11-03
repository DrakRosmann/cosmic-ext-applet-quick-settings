// SPDX-License-Identifier: GPL-3.0-only

//! The system services behind the controls: NetworkManager, BlueZ, UPower,
//! power-profiles-daemon, PipeWire (through `wpctl`), cosmic-settings-daemon
//! for brightness, and COSMIC's own settings.
//!
//! Each service is read again whenever it says something changed.

use std::{collections::HashMap, time::Duration};

use cosmic::{
    cosmic_config::{Config, ConfigGet, ConfigSet},
    iced::{Subscription, stream},
};
use futures::{SinkExt, StreamExt, channel::mpsc};
use tokio::{io::AsyncBufReadExt, process::Command};
use tokio::sync::OnceCell;

use crate::latest::Latest;
use zbus::{
    MatchRule, MessageStream, fdo,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Network {
    pub wifi_enabled: bool,
    /// Name and kind of the connection in use: `802-11-wireless`, `802-3-ethernet`…
    pub connection: Option<(String, String)>,
    /// The VPNs set up, WireGuard's too, the last used first.
    pub vpns: Vec<Vpn>,
    /// The DNS servers of the Wi-Fi or cable in use.
    pub dns: Option<crate::dns::Dns>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vpn {
    /// Its saved connection.
    pub path: OwnedObjectPath,
    pub name: String,
    /// The active connection, while on or connecting.
    pub active: Option<OwnedObjectPath>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bluetooth {
    pub adapter: Option<OwnedObjectPath>,
    pub powered: bool,
    pub connected: Vec<String>,
    /// Paired devices.
    pub devices: Vec<BluetoothDevice>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BluetoothDevice {
    pub path: OwnedObjectPath,
    pub name: String,
    pub icon: String,
    pub connected: bool,
    /// Percent, when the device tells BlueZ.
    pub battery: Option<u8>,
}

/// A wireless mouse, keyboard, headset… with a battery.
#[derive(Clone, Debug, PartialEq)]
pub struct Peripheral {
    pub name: String,
    pub icon: &'static str,
    pub percentage: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Battery {
    pub percentage: f64,
    pub charging: bool,
    pub icon: String,
    /// Seconds until empty, or until full when charging, once UPower knows.
    pub time_left: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Volume {
    /// 0.0 to 1.0, more when boosted.
    pub level: f32,
    pub muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PowerProfiles {
    pub active: String,
    pub available: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Sink {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    Network(Network),
    Bluetooth(Bluetooth),
    Battery(Option<Battery>),
    Volume(Option<Volume>),
    /// Current and maximum.
    Brightness(Option<(i32, i32)>),
    PowerProfiles(Option<PowerProfiles>),
    /// Whether the default microphone is muted; `None` without one.
    Microphone(Option<bool>),
    Peripherals(Vec<Peripheral>),
    /// Apps' streams or the microphones changed.
    Audio,
}

#[derive(Clone, Copy, Debug, Hash)]
enum Service {
    Network,
    Bluetooth,
    Battery,
    Peripherals,
    PowerProfiles,
    Brightness,
}

impl Service {
    fn name(self) -> &'static str {
        match self {
            Self::Network => "org.freedesktop.NetworkManager",
            Self::Bluetooth => "org.bluez",
            Self::Battery | Self::Peripherals => "org.freedesktop.UPower",
            Self::PowerProfiles => "net.hadess.PowerProfiles",
            Self::Brightness => "com.system76.CosmicSettingsDaemon",
        }
    }

    fn rules(self) -> Vec<MatchRule<'static>> {
        let name = self.name();
        vec![
            match self {
                // Devices come and go, too.
                Self::Bluetooth | Self::Peripherals => signals_from(name),
                _ => properties_changed(name),
            },
            // It starting, late at login (COSMIC's settings daemon often
            // comes up after the panel), or again after a crash: it says
            // nothing else then.
            owner_changed(name),
        ]
    }

    async fn bus(self) -> zbus::Result<zbus::Connection> {
        match self {
            Self::Brightness => zbus::Connection::session().await,
            _ => zbus::Connection::system().await,
        }
    }

    async fn read(self, conn: &zbus::Connection) -> Update {
        match self {
            Self::Network => Update::Network(network(conn).await.unwrap_or_default()),
            Self::Bluetooth => Update::Bluetooth(bluetooth(conn).await.unwrap_or_default()),
            Self::Battery => Update::Battery(battery(conn).await.ok().flatten()),
            Self::Peripherals => Update::Peripherals(peripherals(conn).await.unwrap_or_default()),
            Self::PowerProfiles => Update::PowerProfiles(power_profiles(conn).await.ok()),
            Self::Brightness => Update::Brightness(brightness(conn).await.ok().flatten()),
        }
    }
}

/// Read a service now and again whenever it says something changed.
fn watch(service: Service) -> Subscription<Update> {
    Subscription::run_with(service, |service| {
        let service = *service;
        stream::channel(4, move |mut output: mpsc::Sender<Update>| async move {
            let Ok(conn) = service.bus().await else {
                tracing::warn!("{service:?}: no bus");
                return futures::future::pending().await;
            };
            let mut streams = Vec::new();
            for rule in service.rules() {
                match MessageStream::for_match_rule(rule, &conn, Some(64)).await {
                    Ok(stream) => streams.push(stream),
                    Err(err) => tracing::warn!("{service:?}: {err}"),
                }
            }
            let mut changes = futures::stream::select_all(streams);
            loop {
                if output.send(service.read(&conn).await).await.is_err() {
                    return;
                }
                // Read again now and then even if nothing is said: a change
                // missed, or a read that failed, isn't kept for long.
                match tokio::time::timeout(Duration::from_secs(30), changes.next()).await {
                    Ok(None) => return futures::future::pending().await,
                    Ok(Some(_)) | Err(_) => {}
                }
                // Changes come in bursts.
                tokio::time::sleep(Duration::from_millis(100)).await;
                while futures::FutureExt::now_or_never(changes.next())
                    .flatten()
                    .is_some()
                {}
            }
        })
    })
}

fn properties_changed(sender: &'static str) -> MatchRule<'static> {
    MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(sender)
        .unwrap()
        .interface("org.freedesktop.DBus.Properties")
        .unwrap()
        .member("PropertiesChanged")
        .unwrap()
        .build()
}

fn owner_changed(name: &'static str) -> MatchRule<'static> {
    MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .unwrap()
        .interface("org.freedesktop.DBus")
        .unwrap()
        .member("NameOwnerChanged")
        .unwrap()
        .arg(0, name)
        .unwrap()
        .build()
}

fn signals_from(sender: &'static str) -> MatchRule<'static> {
    MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(sender)
        .unwrap()
        .build()
}

pub fn subscription() -> Subscription<Update> {
    Subscription::batch([
        watch(Service::Network),
        watch(Service::Bluetooth),
        watch(Service::Battery),
        watch(Service::Peripherals),
        watch(Service::PowerProfiles),
        watch(Service::Brightness),
        volume_subscription(),
    ])
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wireless_enabled(&self, enabled: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn connection(&self) -> zbus::Result<OwnedObjectPath>;
    /// 2 once activated.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

async fn network(conn: &zbus::Connection) -> zbus::Result<Network> {
    // Each part on its own: one failing, say the primary connection gone
    // between asking for it and reading it while NetworkManager switches,
    // used to leave everything at its default, Wi-Fi off, until the next
    // change, which on a quiet network may not come for long.
    let nm = NetworkManagerProxy::builder(conn)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let active = active_connections(conn, &nm).await;
    let connection = active.first().map(|a| (a.name.clone(), a.kind.clone()));
    // Connected through Wi-Fi, it is on, whatever else says.
    let on_wifi = active.iter().any(|a| a.kind == "802-11-wireless");
    let wifi_enabled = match nm.wireless_enabled().await {
        Ok(enabled) => enabled || on_wifi,
        Err(err) => {
            tracing::warn!("cannot read whether Wi-Fi is on: {err}");
            on_wifi || wifi_device(conn).await.ok().flatten().is_some()
        }
    };
    // The servers of the Wi-Fi or cable in use, not of a VPN on top.
    let dns = match active.iter().find(|a| a.physical()) {
        Some(a) => crate::dns::current(conn, a.path.clone(), a.settings.clone()).await.ok(),
        None => None,
    };
    Ok(Network {
        wifi_enabled,
        connection,
        vpns: vpns(conn, &nm).await.unwrap_or_default(),
        dns,
    })
}

/// The network's state now, for a page opening: what it shows must not wait
/// for NetworkManager to say something changed.
pub async fn network_now() -> Option<Network> {
    let conn = zbus::Connection::system().await.ok()?;
    network(&conn).await.ok()
}

struct ActiveInfo {
    path: OwnedObjectPath,
    settings: OwnedObjectPath,
    name: String,
    kind: String,
}

impl ActiveInfo {
    fn physical(&self) -> bool {
        matches!(self.kind.as_str(), "802-11-wireless" | "802-3-ethernet")
    }
}

/// The connections up: the primary one first; with none, the Wi-Fi or
/// cable ones, never a bridge or loopback NetworkManager keeps for itself.
async fn active_connections(conn: &zbus::Connection, nm: &NetworkManagerProxy<'_>) -> Vec<ActiveInfo> {
    let read = |path: OwnedObjectPath| async move {
        let active = ActiveConnectionProxy::builder(conn)
            .path(path.clone())?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        zbus::Result::Ok((
            active.state().await.unwrap_or(0),
            ActiveInfo {
                settings: active.connection().await?,
                name: active.id().await?,
                kind: active.kind().await?,
                path,
            },
        ))
    };
    let mut found = Vec::new();
    let primary = nm.primary_connection().await.ok().filter(|p| p.as_str() != "/");
    if let Some(primary) = primary.clone() {
        if let Ok((_, info)) = read(primary).await {
            found.push(info);
        }
    }
    for path in nm.active_connections().await.unwrap_or_default() {
        if Some(&path) == primary.as_ref() {
            continue;
        }
        // 2: activated.
        if let Ok((2, info)) = read(path).await {
            if info.physical() {
                found.push(info);
            }
        }
    }
    found
}

async fn vpns(conn: &zbus::Connection, nm: &NetworkManagerProxy<'_>) -> zbus::Result<Vec<Vpn>> {
    // Saved connection to active one.
    let mut active = HashMap::new();
    for path in nm.active_connections().await? {
        let proxy = ActiveConnectionProxy::builder(conn).path(path.clone())?.build().await?;
        if let Ok(connection) = proxy.connection().await {
            active.insert(connection, path);
        }
    }
    let mut vpns = Vec::new();
    for path in NmSettingsProxy::new(conn).await?.list_connections().await? {
        let connection = NmConnectionProxy::builder(conn).path(path.clone())?.build().await?;
        let Ok(settings) = connection.get_settings().await else {
            continue;
        };
        let Some(about) = settings.get("connection") else {
            continue;
        };
        let text = |key: &str| about.get(key).and_then(|v| String::try_from(v.clone()).ok());
        if !matches!(text("type").as_deref(), Some("vpn" | "wireguard")) {
            continue;
        }
        let used = about
            .get("timestamp")
            .and_then(|v| u64::try_from(v.clone()).ok())
            .unwrap_or(0);
        vpns.push((
            used,
            Vpn {
                active: active.remove(&path),
                name: text("id").unwrap_or_default(),
                path,
            },
        ));
    }
    vpns.sort_by_key(|(used, vpn)| (std::cmp::Reverse(*used), vpn.name.clone()));
    Ok(vpns.into_iter().map(|(_, vpn)| vpn).collect())
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManagerMethods {
    fn get_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn deactivate_connection(&self, active: &ObjectPath<'_>) -> zbus::Result<()>;
    fn activate_connection(
        &self,
        connection: &ObjectPath<'_>,
        device: &ObjectPath<'_>,
        specific_object: &ObjectPath<'_>,
    ) -> zbus::Result<OwnedObjectPath>;
    fn add_and_activate_connection(
        &self,
        connection: HashMap<&str, HashMap<&str, Value<'_>>>,
        device: &ObjectPath<'_>,
        specific_object: &ObjectPath<'_>,
    ) -> zbus::Result<(OwnedObjectPath, OwnedObjectPath)>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NmDevice {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NmWireless {
    fn request_scan(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    #[zbus(property)]
    fn access_points(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NmAccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;
    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
    #[zbus(property)]
    fn flags(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn wpa_flags(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn rsn_flags(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Settings"
)]
trait NmSettings {
    fn list_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NmConnection {
    fn get_settings(&self) -> zbus::Result<HashMap<String, HashMap<String, OwnedValue>>>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccessPoint {
    pub path: OwnedObjectPath,
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
    pub active: bool,
    /// A saved connection for it.
    pub saved: Option<OwnedObjectPath>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WifiNetworks {
    pub device: OwnedObjectPath,
    pub networks: Vec<AccessPoint>,
}

async fn wifi_device(conn: &zbus::Connection) -> zbus::Result<Option<OwnedObjectPath>> {
    for device in NetworkManagerMethodsProxy::new(conn).await?.get_devices().await? {
        let proxy = NmDeviceProxy::builder(conn).path(device.clone())?.build().await?;
        // NM_DEVICE_TYPE_WIFI
        if proxy.device_type().await? == 2 {
            return Ok(Some(device));
        }
    }
    Ok(None)
}

/// Saved Wi-Fi connections, by SSID.
async fn saved_wifi(conn: &zbus::Connection) -> zbus::Result<HashMap<String, OwnedObjectPath>> {
    let mut saved = HashMap::new();
    for path in NmSettingsProxy::new(conn).await?.list_connections().await? {
        let connection = NmConnectionProxy::builder(conn).path(path.clone())?.build().await?;
        let Ok(settings) = connection.get_settings().await else {
            continue;
        };
        let ssid = settings
            .get("802-11-wireless")
            .and_then(|w| w.get("ssid"))
            .and_then(|v| Vec::<u8>::try_from(v.clone()).ok());
        if let Some(ssid) = ssid {
            saved.insert(String::from_utf8_lossy(&ssid).into_owned(), path);
        }
    }
    Ok(saved)
}

/// The networks in reach, the one in use first, then saved ones, then by signal.
pub async fn wifi_networks(scan: bool) -> zbus::Result<Option<WifiNetworks>> {
    let conn = zbus::Connection::system().await?;
    let Some(device) = wifi_device(&conn).await? else {
        return Ok(None);
    };
    let wireless = NmWirelessProxy::builder(&conn)
        .path(device.clone())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    if scan {
        // Fails when a scan ran a moment ago; the list is fresh then anyway.
        _ = wireless.request_scan(HashMap::new()).await;
    }
    let active = wireless.active_access_point().await?;
    let saved = saved_wifi(&conn).await.unwrap_or_default();
    let mut networks: Vec<AccessPoint> = Vec::new();
    for path in wireless.access_points().await? {
        let ap = NmAccessPointProxy::builder(&conn).path(path.clone())?.build().await?;
        let ssid = String::from_utf8_lossy(&ap.ssid().await.unwrap_or_default()).into_owned();
        if ssid.is_empty() {
            continue;
        }
        let strength = ap.strength().await.unwrap_or(0);
        let secure = ap.flags().await.unwrap_or(0) & 1 != 0
            || ap.wpa_flags().await.unwrap_or(0) != 0
            || ap.rsn_flags().await.unwrap_or(0) != 0;
        let is_active = path == active;
        // One entry per network, from its strongest access point.
        if let Some(existing) = networks.iter_mut().find(|n| n.ssid == ssid) {
            existing.active |= is_active;
            if strength > existing.strength {
                existing.strength = strength;
                existing.path = path;
            }
            continue;
        }
        networks.push(AccessPoint {
            saved: saved.get(&ssid).cloned(),
            path,
            ssid,
            strength,
            secure,
            active: is_active,
        });
    }
    networks.sort_by_key(|n| (!n.active, n.saved.is_none(), std::cmp::Reverse(n.strength)));
    Ok(Some(WifiNetworks { device, networks }))
}

pub enum Joined {
    Connecting,
    /// Needs a password, which only COSMIC Settings asks for.
    NeedsSettings,
}

pub async fn join_wifi(device: OwnedObjectPath, network: AccessPoint) -> zbus::Result<Joined> {
    let conn = zbus::Connection::system().await?;
    let nm = NetworkManagerMethodsProxy::new(&conn).await?;
    if let Some(saved) = &network.saved {
        nm.activate_connection(saved, &device, &network.path).await?;
        return Ok(Joined::Connecting);
    }
    if network.secure {
        return Ok(Joined::NeedsSettings);
    }
    nm.add_and_activate_connection(HashMap::new(), &device, &network.path)
        .await?;
    Ok(Joined::Connecting)
}

/// Turns a VPN on, from its saved connection, or off, from its active one.
pub async fn connect_vpn(vpn: Vpn) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let nm = NetworkManagerMethodsProxy::new(&conn).await?;
    let none = ObjectPath::from_static_str_unchecked("/");
    match &vpn.active {
        Some(active) => nm.deactivate_connection(active).await,
        None => nm.activate_connection(&vpn.path, &none, &none).await.map(|_| ()),
    }
}

pub async fn set_wifi(enabled: bool) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    NetworkManagerProxy::new(&conn)
        .await?
        .set_wireless_enabled(enabled)
        .await
}

type ManagedObjects =
    HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

async fn bluetooth(conn: &zbus::Connection) -> zbus::Result<Bluetooth> {
    let manager = fdo::ObjectManagerProxy::builder(conn)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;
    let objects: ManagedObjects = manager
        .get_managed_objects()
        .await?
        .into_iter()
        .map(|(path, interfaces)| {
            let interfaces = interfaces
                .into_iter()
                .map(|(name, props)| (name.to_string(), props))
                .collect();
            (path, interfaces)
        })
        .collect();
    let flag = |props: &HashMap<String, OwnedValue>, key: &str| {
        props
            .get(key)
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false)
    };
    let mut status = Bluetooth::default();
    for (path, interfaces) in &objects {
        if let Some(adapter) = interfaces.get("org.bluez.Adapter1") {
            if status.adapter.is_none() {
                status.adapter = Some(path.clone());
                status.powered = flag(adapter, "Powered");
            }
        }
        if let Some(device) = interfaces.get("org.bluez.Device1") {
            let text = |key: &str| {
                device
                    .get(key)
                    .and_then(|v| String::try_from(v.clone()).ok())
            };
            let name = text("Alias").or_else(|| text("Name")).unwrap_or_default();
            let connected = flag(device, "Connected");
            if connected {
                status.connected.push(name.clone());
            }
            if flag(device, "Paired") {
                let battery = interfaces
                    .get("org.bluez.Battery1")
                    .and_then(|b| b.get("Percentage"))
                    .and_then(|v| u8::try_from(v).ok());
                status.devices.push(BluetoothDevice {
                    path: path.clone(),
                    name,
                    icon: text("Icon").unwrap_or_else(|| "bluetooth".to_owned()),
                    connected,
                    battery,
                });
            }
        }
    }
    status.devices.sort_by(|a, b| (!a.connected, &a.name).cmp(&(!b.connected, &b.name)));
    Ok(status)
}

pub async fn set_bluetooth(adapter: OwnedObjectPath, powered: bool) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let properties = fdo::PropertiesProxy::builder(&conn)
        .destination("org.bluez")?
        .path(adapter)?
        .build()
        .await?;
    properties
        .set(
            "org.bluez.Adapter1".try_into()?,
            "Powered",
            zbus::zvariant::Value::Bool(powered),
        )
        .await?;
    Ok(())
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait BluezDevice {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
}

pub async fn connect_bluetooth(device: OwnedObjectPath, connect: bool) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let device = BluezDeviceProxy::builder(&conn).path(device)?.build().await?;
    match connect {
        true => device.connect().await,
        false => device.disconnect().await,
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait Device {
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    fn enumerate_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
trait UPowerDevice {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn power_supply(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;
}

/// The icon of a kind of UPower device.
fn peripheral_icon(kind: u32) -> &'static str {
    match kind {
        5 => "input-mouse-symbolic",
        6 => "input-keyboard-symbolic",
        8 => "phone-symbolic",
        12 | 22 => "input-tablet-symbolic",
        17 => "audio-headset-symbolic",
        18 => "audio-speakers-symbolic",
        19 => "audio-headphones-symbolic",
        21 => "input-gaming-symbolic",
        23 => "input-touchpad-symbolic",
        _ => "battery-symbolic",
    }
}

async fn peripherals(conn: &zbus::Connection) -> zbus::Result<Vec<Peripheral>> {
    let mut peripherals: Vec<Peripheral> = Vec::new();
    for path in UPowerProxy::new(conn).await?.enumerate_devices().await? {
        let device = UPowerDeviceProxy::builder(conn)
            .path(path)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        // Not the computer's own battery or charger.
        if device.power_supply().await.unwrap_or(true) || !device.is_present().await.unwrap_or(false) {
            continue;
        }
        let name = device.model().await.unwrap_or_default();
        // The same device through two ways, say Bluetooth and Logitech's protocol.
        if peripherals.iter().any(|p| p.name == name) {
            continue;
        }
        peripherals.push(Peripheral {
            icon: peripheral_icon(device.kind().await.unwrap_or(0)),
            percentage: device.percentage().await.unwrap_or(0.0),
            name,
        });
    }
    peripherals.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(peripherals)
}

/// Adwaita's battery icon for a level, in tens, and UPower's state: 1
/// charging, 4 fully charged, 5 plugged in but waiting to charge.
fn battery_level_icon(percentage: f64, state: u32) -> String {
    let level = ((percentage.clamp(0.0, 100.0) / 10.0).round() as u32 * 10).min(100);
    match state {
        4 if level == 100 => "battery-level-100-charged-symbolic".to_owned(),
        1 => format!("battery-level-{level}-charging-symbolic"),
        4 | 5 => format!("battery-level-{level}-plugged-in-symbolic"),
        _ => format!("battery-level-{level}-symbolic"),
    }
}

async fn battery(conn: &zbus::Connection) -> zbus::Result<Option<Battery>> {
    let device = DeviceProxy::builder(conn)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    if !device.is_present().await? {
        return Ok(None);
    }
    let state = device.state().await?;
    // 1 charging, 2 discharging, 4 fully charged
    let time_left = match state {
        1 => device.time_to_full().await.ok(),
        2 => device.time_to_empty().await.ok(),
        _ => None,
    }
    .filter(|t| *t > 0);
    let percentage = device.percentage().await?;
    Ok(Some(Battery {
        percentage,
        charging: matches!(state, 1 | 4),
        // UPower's own name says only full, good, low, caution or empty, and
        // many themes draw "good" as a full battery: at 40% the panel showed
        // it full. Drawn from the level instead, as the battery applet does.
        icon: battery_level_icon(percentage, state),
        time_left,
    }))
}

#[zbus::proxy(
    interface = "net.hadess.PowerProfiles",
    default_service = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
trait PowerProfilesDaemon {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
}

async fn power_profiles(conn: &zbus::Connection) -> zbus::Result<PowerProfiles> {
    let daemon = PowerProfilesDaemonProxy::builder(conn)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let available = daemon
        .profiles()
        .await?
        .iter()
        .filter_map(|p| p.get("Profile").and_then(|v| String::try_from(v.clone()).ok()))
        .collect();
    Ok(PowerProfiles {
        active: daemon.active_profile().await?,
        available,
    })
}

pub async fn set_power_profile(profile: String) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    PowerProfilesDaemonProxy::new(&conn)
        .await?
        .set_active_profile(&profile)
        .await
}

#[zbus::proxy(
    interface = "com.system76.CosmicSettingsDaemon",
    default_service = "com.system76.CosmicSettingsDaemon",
    default_path = "/com/system76/CosmicSettingsDaemon"
)]
trait SettingsDaemon {
    #[zbus(property)]
    fn display_brightness(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn set_display_brightness(&self, value: i32) -> zbus::Result<()>;
    #[zbus(property)]
    fn max_display_brightness(&self) -> zbus::Result<i32>;
}

async fn brightness(conn: &zbus::Connection) -> zbus::Result<Option<(i32, i32)>> {
    let daemon = SettingsDaemonProxy::builder(conn)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let max = daemon.max_display_brightness().await?;
    if max <= 0 {
        return Ok(None);
    }
    Ok(Some((daemon.display_brightness().await?, max)))
}

pub async fn set_brightness(value: i32) {
    static LATEST: Latest<(), i32> = Latest::new();
    LATEST
        .send((), value, |(), value| async move {
            let set = async {
                let conn = zbus::Connection::session().await?;
                SettingsDaemonProxy::new(&conn)
                    .await?
                    .set_display_brightness(value)
                    .await
            };
            if let Err(err) = set.await {
                tracing::warn!("cannot set brightness: {err}");
            }
        })
        .await;
}

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

/// `Volume: 0.45 [MUTED]`
fn parse_volume(output: &str) -> Option<Volume> {
    let rest = output.trim().strip_prefix("Volume:")?.trim();
    let level = rest.split_whitespace().next()?.parse().ok()?;
    Some(Volume {
        level,
        muted: rest.contains("[MUTED]"),
    })
}

async fn volume() -> Option<Volume> {
    get_volume(SINK).await
}

async fn microphone() -> Option<bool> {
    get_volume(SOURCE).await.map(|v| v.muted)
}

async fn get_volume(node: &str) -> Option<Volume> {
    let output = crate::host::command("wpctl", &[])
        .args(["get-volume", node])
        .output()
        .await
        .ok()?;
    parse_volume(&String::from_utf8_lossy(&output.stdout))
}

pub async fn set_volume(level: f32) {
    static LATEST: Latest<(), f32> = Latest::new();
    LATEST
        .send((), level, |(), level| async move {
            let level = format!("{:.2}", level.clamp(0.0, 1.0));
            _ = crate::host::command("wpctl", &[])
                .args(["set-volume", SINK, &level])
                .status()
                .await;
        })
        .await;
}

pub async fn toggle_mute() {
    _ = crate::host::command("wpctl", &[])
        .args(["set-mute", SINK, "toggle"])
        .status()
        .await;
}

pub async fn toggle_microphone() {
    _ = crate::host::command("wpctl", &[])
        .args(["set-mute", SOURCE, "toggle"])
        .status()
        .await;
}

pub(crate) fn pactl() -> Command {
    // Its JSON breaks on translated messages.
    crate::host::command("pactl", &[("LC_ALL", "C")])
}

/// `pactl subscribe`, which ends with the applet: left behind, each would
/// hold one of the few connections PipeWire allows.
pub(crate) fn pactl_subscribe() -> Command {
    let mut pactl = pactl();
    pactl.arg("subscribe");
    // SAFETY: prctl is async-signal-safe.
    unsafe {
        pactl.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    pactl
}

/// Audio outputs, and the name of the one in use.
pub async fn sinks() -> Option<(Vec<Sink>, String)> {
    let list = pactl()
        .args(["-f", "json", "list", "sinks"])
        .output()
        .await
        .ok()?;
    let list: Vec<serde_json::Value> = serde_json::from_slice(&list.stdout).ok()?;
    let default = pactl().arg("get-default-sink").output().await.ok()?;
    let default = String::from_utf8_lossy(&default.stdout).trim().to_owned();
    let sinks = list
        .iter()
        .filter_map(|sink| {
            let name = sink["name"].as_str()?;
            let active = sink["active_port"].as_str();
            let port = sink["ports"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|port| port["name"].as_str() == active);
            // Nothing plugged into it, like an HDMI port without a screen.
            let unplugged = port.is_some_and(|p| p["availability"] == "not available");
            if unplugged && name != default {
                return None;
            }
            // The port says it shortest, "Speaker" or "HDMI / DisplayPort 1
            // Output", for a sound card's outputs; a device by its name.
            let description = port
                .filter(|_| !name.starts_with("bluez"))
                .and_then(|p| p["description"].as_str())
                .or(sink["description"].as_str())?;
            Some(Sink {
                name: name.to_owned(),
                description: description.to_owned(),
            })
        })
        .collect();
    Some((sinks, default))
}

pub async fn set_sink(name: String) {
    _ = pactl().args(["set-default-sink", &name]).status().await;
}

/// PipeWire tells about changes through its PulseAudio side.
fn volume_subscription() -> Subscription<Update> {
    Subscription::run_with("volume", |_| {
        stream::channel(4, |mut output: mpsc::Sender<Update>| async move {
            loop {
                _ = output.send(Update::Volume(volume().await)).await;
                _ = output.send(Update::Microphone(microphone().await)).await;
                let Ok(mut child) = pactl_subscribe()
                    .stdout(std::process::Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()
                else {
                    return futures::future::pending().await;
                };
                let Some(stdout) = child.stdout.take() else {
                    return futures::future::pending().await;
                };
                let mut lines = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("sink") || line.contains("server") {
                        if output.send(Update::Volume(volume().await)).await.is_err() {
                            return;
                        }
                    }
                    if line.contains("sink-input") || line.contains("on source") {
                        if output.send(Update::Audio).await.is_err() {
                            return;
                        }
                    }
                    if line.contains("source") || line.contains("server") {
                        if output.send(Update::Microphone(microphone().await)).await.is_err() {
                            return;
                        }
                    }
                }
                // pactl went away, say PipeWire restarted.
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        })
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Display {
    /// Connector, like `eDP-1` or `HDMI-A-1`.
    pub name: String,
    pub make: String,
    pub model: String,
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
    /// Millihertz.
    pub refresh: u32,
    pub scale: f32,
    /// Its own brightness control over DDC/CI: display number, current, maximum.
    pub ddc: Option<(u32, u32, u32)>,
}

impl Display {
    /// The laptop's own screen.
    pub fn internal(&self) -> bool {
        self.name.starts_with("eDP") || self.name.starts_with("LVDS") || self.name.starts_with("DSI")
    }
}

/// `cosmic-randr list --kdl`, which reads:
///
/// ```text
/// output "eDP-1" enabled=#true {
///   description make="AU Optronics" model="0xE3AC"
///   scale 1.25
///   modes {
///     mode 1920 1200 60001 current=#true preferred=#true
/// ```
fn parse_displays(kdl: &str) -> Vec<Display> {
    let quoted = |line: &str, key: &str| -> Option<String> {
        let rest = line.split(&format!("{key}=\"")).nth(1)?;
        Some(rest.split('"').next()?.to_owned())
    };
    let mut displays: Vec<Display> = Vec::new();
    for line in kdl.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("output \"") {
            displays.push(Display {
                name: rest.split('"').next().unwrap_or_default().to_owned(),
                enabled: line.contains("enabled=#true"),
                scale: 1.0,
                ..Display::default()
            });
            continue;
        }
        let Some(display) = displays.last_mut() else {
            continue;
        };
        if line.starts_with("description") {
            display.make = quoted(line, "make").unwrap_or_default();
            display.model = quoted(line, "model").unwrap_or_default();
        } else if let Some(scale) = line.strip_prefix("scale ") {
            display.scale = scale.trim().parse().unwrap_or(1.0);
        } else if line.starts_with("mode ") && line.contains("current=#true") {
            let numbers: Vec<u32> = line
                .split_whitespace()
                .skip(1)
                .take(3)
                .filter_map(|n| n.parse().ok())
                .collect();
            if let [width, height, refresh] = numbers[..] {
                (display.width, display.height, display.refresh) = (width, height, refresh);
            }
        }
    }
    displays
}

/// External monitors `ddcutil` can talk to, by connector: their display numbers.
async fn ddc_displays() -> HashMap<String, u32> {
    let Ok(output) = crate::host::command("ddcutil", &[])
        .args(["detect", "--brief"])
        .output()
        .await
    else {
        return HashMap::new();
    };
    let mut displays = HashMap::new();
    let mut number = None;
    for line in String::from_utf8_lossy(&output.stdout).lines().map(str::trim) {
        if let Some(n) = line.strip_prefix("Display ") {
            number = n.trim().parse().ok();
        } else if let Some(connector) = line.strip_prefix("DRM connector:") {
            // `card1-HDMI-A-1`
            let connector = connector.trim();
            let connector = connector.split_once('-').map_or(connector, |(_, c)| c);
            if let Some(n) = number {
                displays.insert(connector.to_owned(), n);
            }
        }
    }
    displays
}

/// `VCP 10 C 50 100`: current and maximum.
fn parse_vcp(output: &str) -> Option<(u32, u32)> {
    let fields: Vec<&str> = output.split_whitespace().collect();
    match fields[..] {
        ["VCP", _, "C", current, max, ..] => Some((current.parse().ok()?, max.parse().ok()?)),
        _ => None,
    }
}

pub async fn displays() -> Vec<Display> {
    let Ok(output) = crate::host::command("cosmic-randr", &[])
        .args(["list", "--kdl"])
        .output()
        .await
    else {
        return Vec::new();
    };
    let mut displays = parse_displays(&String::from_utf8_lossy(&output.stdout));
    if displays.iter().any(|d| d.enabled && !d.internal()) {
        let ddc = ddc_displays().await;
        for display in &mut displays {
            let Some(&number) = ddc.get(&display.name) else {
                continue;
            };
            let Ok(vcp) = crate::host::command("ddcutil", &[])
                .args(["--display", &number.to_string(), "getvcp", "10", "--brief"])
                .output()
                .await
            else {
                continue;
            };
            if let Some((current, max)) = parse_vcp(&String::from_utf8_lossy(&vcp.stdout)) {
                display.ddc = Some((number, current, max));
            }
        }
    }
    displays
}

pub async fn set_scale(display: Display, scale: f32) {
    _ = crate::host::command("cosmic-randr", &[])
        .args([
            "mode".to_owned(),
            display.name,
            display.width.to_string(),
            display.height.to_string(),
            "--refresh".to_owned(),
            format!("{:.3}", display.refresh as f32 / 1000.0),
            "--scale".to_owned(),
            scale.to_string(),
        ])
        .status()
        .await;
}

pub async fn set_ddc_brightness(number: u32, value: u32) {
    _ = crate::host::command("ddcutil", &[])
        .args(["--display", &number.to_string(), "setvcp", "10", &value.to_string()])
        .status()
        .await;
}

/// cosmic-comp's settings, kept as cosmic-comp-config does without needing
/// it, and libinput with it.
const COMP_CONFIG_VERSION: u64 = 1;

/// Whether tiling follows each workspace, as cosmic-comp-config says it.
#[derive(serde::Serialize)]
enum TileBehavior {
    PerWorkspace,
}

/// cosmic-comp's tiling settings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TilingConfig {
    /// New workspaces tile their windows.
    pub new_workspaces_tile: bool,
    /// The active window is outlined.
    pub active_hint: bool,
}

fn comp_config() -> Result<Config, cosmic::cosmic_config::Error> {
    Config::new(
        "com.system76.CosmicComp",
        COMP_CONFIG_VERSION,
    )
}

pub fn tiling_config() -> TilingConfig {
    let Ok(config) = comp_config() else {
        return TilingConfig::default();
    };
    TilingConfig {
        new_workspaces_tile: config.get("autotile").unwrap_or(false),
        active_hint: config.get("active_hint").unwrap_or(true),
    }
}

pub fn set_tiling_config(key: &str, value: bool) {
    let result = comp_config().and_then(|config| {
        // Tiling one workspace takes cosmic-comp tiling per workspace, as the
        // tiling applet does.
        config.set(
            "autotile_behavior",
            TileBehavior::PerWorkspace,
        )?;
        config.set(key, value)
    });
    if let Err(err) = result {
        tracing::warn!("cannot set {key}: {err}");
    }
}

/// Switch high contrast on or off in both themes, as COSMIC's accessibility
/// applet does.
pub fn set_high_contrast(enabled: bool) {
    use cosmic::{
        cosmic_config::CosmicConfigEntry,
        cosmic_theme::{CosmicPalette, Theme as CosmicTheme, ThemeBuilder},
    };
    std::thread::spawn(move || {
        for dark in [true, false] {
            let result = (|| {
                let builder_config = if dark {
                    ThemeBuilder::dark_config()?
                } else {
                    ThemeBuilder::light_config()?
                };
                let mut builder = ThemeBuilder::get_entry(&builder_config).unwrap_or_else(|(_, b)| b);
                let palette = builder.palette.inner();
                builder.palette = match (dark, enabled) {
                    (true, true) => CosmicPalette::HighContrastDark(palette),
                    (true, false) => CosmicPalette::Dark(palette),
                    (false, true) => CosmicPalette::HighContrastLight(palette),
                    (false, false) => CosmicPalette::Light(palette),
                };
                builder.write_entry(&builder_config)?;
                let theme_config = if dark {
                    CosmicTheme::dark_config()?
                } else {
                    CosmicTheme::light_config()?
                };
                builder.build().write_entry(&theme_config)?;
                Ok::<(), cosmic::cosmic_config::Error>(())
            })();
            if let Err(err) = result {
                tracing::warn!("cannot set high contrast: {err:?}");
            }
        }
    });
}

pub use cosmic_settings_a11y_manager_subscription::{
    AccessibilityEvent, AccessibilityRequest, ColorFilter,
};

#[derive(Clone, Debug)]
pub enum A11y {
    Ready(cosmic::cctk::sctk::reexports::calloop::channel::Sender<AccessibilityRequest>),
    Event(AccessibilityEvent),
}

/// The magnifier and screen filters, through COSMIC's accessibility protocol.
pub fn a11y_subscription() -> Subscription<A11y> {
    Subscription::run_with("a11y", |_| {
        stream::channel(8, |mut output: mpsc::Sender<A11y>| async move {
            match cosmic_settings_a11y_manager_subscription::spawn_wayland_connection(1) {
                Ok((tx, mut rx)) => {
                    _ = output.send(A11y::Ready(tx)).await;
                    while let Some(event) = rx.recv().await {
                        if output.send(A11y::Event(event)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(err) => tracing::warn!("no accessibility protocol: {err}"),
            }
            futures::future::pending().await
        })
    })
}

/// The session bus connection inhibitions are tied to: they end with it.
async fn session() -> zbus::Result<zbus::Connection> {
    static BUS: OnceCell<zbus::Connection> = OnceCell::const_new();
    BUS.get_or_try_init(zbus::Connection::session).await.cloned()
}

#[zbus::proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/org/freedesktop/ScreenSaver"
)]
trait ScreenSaver {
    fn inhibit(&self, application: &str, reason: &str) -> zbus::Result<u32>;
    fn un_inhibit(&self, cookie: u32) -> zbus::Result<()>;
}

/// Keep the screen on and the computer awake, until `allow_idle`.
pub async fn inhibit_idle(reason: &str) -> zbus::Result<u32> {
    ScreenSaverProxy::new(&session().await?)
        .await?
        .inhibit(crate::APP_ID, reason)
        .await
}

pub async fn allow_idle(cookie: u32) -> zbus::Result<()> {
    ScreenSaverProxy::new(&session().await?)
        .await?
        .un_inhibit(cookie)
        .await
}

/// Which tiles show, in order, by id.
pub fn tiles() -> Option<Vec<String>> {
    Config::new(crate::APP_ID, 1)
        .and_then(|c| c.get::<Vec<String>>("tiles"))
        .ok()
}

pub fn set_tiles(tiles: &[String]) {
    if let Err(err) = Config::new(crate::APP_ID, 1).and_then(|c| c.set("tiles", tiles)) {
        tracing::warn!("cannot save tiles: {err}");
    }
}

/// Whether the bar shows the computer's battery percentage.
pub const SHOW_PERCENTAGE: Setting = Setting {
    config: crate::APP_ID,
    version: 1,
    key: "show_battery_percentage",
};

/// Fires when COSMIC settings the tiles show change: do not disturb, the
/// theme mode, tiling.
pub fn settings_subscription() -> Subscription<()> {
    Subscription::run_with("settings", |_| {
        stream::channel(4, |output: mpsc::Sender<()>| async move {
            let watchers: Vec<_> = [
                (DO_NOT_DISTURB.config, DO_NOT_DISTURB.version),
                (DARK_MODE.config, DARK_MODE.version),
                (HOT_EDGE, 1),
                (
                    "com.system76.CosmicComp",
                    COMP_CONFIG_VERSION,
                ),
            ]
            .into_iter()
            .filter_map(|(name, version)| {
                let output = std::sync::Mutex::new(output.clone());
                let config = Config::new(name, version).ok()?;
                config
                    .watch(move |_, _| {
                        if let Ok(mut output) = output.lock() {
                            _ = output.try_send(());
                        }
                    })
                    .inspect_err(|err| tracing::warn!("cannot watch {name}: {err}"))
                    .ok()
            })
            .collect();
            futures::future::pending::<()>().await;
            drop(watchers);
        })
    })
}

/// The touchpad, as COSMIC Settings sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touchpad {
    Enabled,
    Disabled,
    /// Off while a mouse is plugged in.
    DisabledOnExternalMouse,
}

/// cosmic-comp's touchpad settings: the user's, or the system's defaults.
/// Read and written as text, changing only their `state`, so that the rest
/// (tapping, scrolling…) stays as it is.
fn touchpad_files() -> (Option<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    use std::path::PathBuf;
    const KEY: &str = "com.system76.CosmicComp/v1/input_touchpad";
    let config = std::env::var_os("HOST_XDG_CONFIG_HOME")
        .filter(|_| crate::host::in_flatpak())
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").filter(|_| !crate::host::in_flatpak()))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let user = config.map(|dir| dir.join("cosmic").join(KEY));
    let system = ["/usr/share/cosmic", "/run/host/usr/share/cosmic"]
        .iter()
        .map(|dir| PathBuf::from(dir).join(KEY))
        .collect();
    (user, system)
}

fn touchpad_text() -> Option<(String, std::path::PathBuf)> {
    let (user, system) = touchpad_files();
    let user = user?;
    let text = std::fs::read_to_string(&user)
        .ok()
        .or_else(|| system.iter().find_map(|path| std::fs::read_to_string(path).ok()))?;
    Some((text, user))
}

/// Where `state:`'s value is in the settings' text.
fn state_value(text: &str) -> Option<std::ops::Range<usize>> {
    let key = text.find("state")?;
    let colon = key + text[key..].find(':')?;
    let start = colon + 1 + text[colon + 1..].len() - text[colon + 1..].trim_start().len();
    let end = start
        + text[start..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(text.len() - start);
    Some(start..end)
}

pub fn touchpad() -> Option<Touchpad> {
    let (text, _) = touchpad_text()?;
    Some(match &text[state_value(&text)?] {
        "Disabled" => Touchpad::Disabled,
        "DisabledOnExternalMouse" => Touchpad::DisabledOnExternalMouse,
        _ => Touchpad::Enabled,
    })
}

pub fn set_touchpad(enabled: bool) {
    let result = (|| {
        let (mut text, path) = touchpad_text().ok_or("no touchpad settings")?;
        let range = state_value(&text).ok_or("no state in the touchpad settings")?;
        text.replace_range(range, if enabled { "Enabled" } else { "Disabled" });
        std::fs::create_dir_all(path.parent().ok_or("no directory")?).map_err(|e| e.to_string())?;
        // Whole, for cosmic-comp, which watches the file.
        let new = path.with_extension("new");
        std::fs::write(&new, text).map_err(|e| e.to_string())?;
        std::fs::rename(&new, &path).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })();
    if let Err(err) = result {
        tracing::warn!("cannot set the touchpad: {err}");
    }
}

/// When do not disturb ends by itself, in seconds since the epoch; 0 never.
pub fn dnd_until() -> Option<i64> {
    Config::new(crate::APP_ID, 1)
        .and_then(|c| c.get::<i64>("do_not_disturb_until"))
        .ok()
        .filter(|t| *t > 0)
}

pub fn set_dnd_until(until: Option<i64>) {
    let result = Config::new(crate::APP_ID, 1)
        .and_then(|c| c.set("do_not_disturb_until", until.unwrap_or(0)));
    if let Err(err) = result {
        tracing::warn!("cannot save when do not disturb ends: {err}");
    }
}

/// The Hot Edge extension (cosmic-ext-hot-edge): its daemon, which makes the
/// edge, and its settings.
const HOT_EDGE: &str = "io.github.DrakRosmann.CosmicHotEdge";
const HOT_EDGE_DAEMON: &str = "cosmic-ext-hot-edge-daemon";

/// Whether the extension is installed: its daemon on the PATH, or where
/// its installers put it.
pub fn hot_edge_installed() -> bool {
    static INSTALLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(str::to_owned)
            .chain([format!("{home}/.local/bin"), "/usr/bin".to_owned(), "/run/host/usr/bin".to_owned()])
            .any(|dir| std::path::Path::new(&dir).join(HOT_EDGE_DAEMON).exists())
    });
    *INSTALLED
}

/// Whether the edge is on: the extension's default, until set.
pub fn hot_edge() -> bool {
    Config::new(HOT_EDGE, 1)
        .and_then(|c| c.get::<bool>("enabled"))
        .unwrap_or(true)
}

/// The daemon stays running while off and follows the setting; on, it is
/// started too, in case it isn't (it runs once per session).
pub fn set_hot_edge(enabled: bool) {
    if let Err(err) = Config::new(HOT_EDGE, 1).and_then(|c| c.set("enabled", enabled)) {
        tracing::warn!("cannot set the hot edge: {err}");
    }
    if enabled {
        crate::host::spawn(HOT_EDGE_DAEMON, &[]);
    }
}

/// Whether the bar leaves out the icons of the tiles that are on.
pub const HIDE_ACTIVE: Setting = Setting {
    config: crate::APP_ID,
    version: 1,
    key: "hide_active_tiles",
};

/// A COSMIC setting, read and written like cosmic-settings does.
pub struct Setting {
    pub config: &'static str,
    pub version: u64,
    pub key: &'static str,
}

pub const DO_NOT_DISTURB: Setting = Setting {
    config: "com.system76.CosmicNotifications",
    version: 1,
    key: "do_not_disturb",
};

pub const DARK_MODE: Setting = Setting {
    config: "com.system76.CosmicTheme.Mode",
    version: 1,
    key: "is_dark",
};

impl Setting {
    pub fn get(&self) -> bool {
        Config::new(self.config, self.version)
            .and_then(|c| c.get::<bool>(self.key))
            .unwrap_or(false)
    }

    pub fn set(&self, value: bool) {
        if let Err(err) =
            Config::new(self.config, self.version).and_then(|c| c.set(self.key, value))
        {
            tracing::warn!("cannot set {}: {err}", self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays() {
        let kdl = r#"output "eDP-1" enabled=#true {
  description make="AU Optronics" model="0xE3AC"
  scale 1.25
  modes {
    mode 1920 1200 60001 current=#true preferred=#true
    mode 1920 1200 40001
  }
}
output "HDMI-A-1" enabled=#false {
  description make="Dell Inc." model="U2419H"
  scale 1
  modes {
    mode 1920 1080 60000 current=#true
  }
}"#;
        let displays = parse_displays(kdl);
        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].name, "eDP-1");
        assert!(displays[0].enabled && displays[0].internal());
        assert_eq!((displays[0].width, displays[0].height, displays[0].refresh), (1920, 1200, 60001));
        assert_eq!(displays[0].scale, 1.25);
        assert_eq!(displays[1].model, "U2419H");
        assert!(!displays[1].enabled && !displays[1].internal());
        assert_eq!(parse_vcp("VCP 10 C 50 100\n"), Some((50, 100)));
    }

    #[test]
    fn volumes() {
        assert_eq!(
            parse_volume("Volume: 0.45\n"),
            Some(Volume {
                level: 0.45,
                muted: false
            })
        );
        assert_eq!(
            parse_volume("Volume: 1.00 [MUTED]\n"),
            Some(Volume {
                level: 1.0,
                muted: true
            })
        );
        assert_eq!(parse_volume(""), None);
    }
}
