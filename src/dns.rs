// SPDX-License-Identifier: GPL-3.0-only

//! The DNS servers of the connection in use: the network's own, or a known
//! provider's, set in NetworkManager as COSMIC Settings would, and applied
//! at once.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::i18n::strings;

/// A known provider, with its IPv4 and IPv6 servers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Provider {
    pub name: &'static str,
    pub ipv4: &'static [&'static str],
    pub ipv6: &'static [&'static str],
}

pub const PROVIDERS: [Provider; 4] = [
    Provider {
        name: "Cloudflare",
        ipv4: &["1.1.1.1", "1.0.0.1"],
        ipv6: &["2606:4700:4700::1111", "2606:4700:4700::1001"],
    },
    Provider {
        name: "Google",
        ipv4: &["8.8.8.8", "8.8.4.4"],
        ipv6: &["2001:4860:4860::8888", "2001:4860:4860::8844"],
    },
    Provider {
        name: "Quad9",
        ipv4: &["9.9.9.9", "149.112.112.112"],
        ipv6: &["2620:fe::fe", "2620:fe::9"],
    },
    Provider {
        name: "AdGuard",
        ipv4: &["94.140.14.14", "94.140.15.15"],
        ipv6: &["2a10:50c0::ad1:ff", "2a10:50c0::ad2:ff"],
    },
];

/// What the connection resolves names with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// The network's own, from DHCP or the router.
    Automatic,
    Provider(Provider),
    /// Servers set by hand, by address.
    Custom(Vec<String>),
}

impl Choice {
    pub fn name(&self) -> String {
        let t = strings();
        match self {
            Choice::Automatic => t.dns_automatic.to_owned(),
            Choice::Provider(provider) => provider.name.to_owned(),
            Choice::Custom(servers) => match servers.first() {
                Some(first) => format!("{} · {first}", t.dns_custom),
                None => t.dns_custom.to_owned(),
            },
        }
    }

    /// What the servers set say, as one of the choices.
    pub fn of(automatic: bool, servers: &[String]) -> Choice {
        if servers.is_empty() {
            return Choice::Automatic;
        }
        let same = |list: &[&str]| {
            let mut a: Vec<&str> = list.to_vec();
            let mut b: Vec<&str> = servers.iter().map(String::as_str).collect();
            a.sort_unstable();
            b.sort_unstable();
            a == b
        };
        match PROVIDERS.iter().find(|p| same(p.ipv4)) {
            Some(provider) if !automatic => Choice::Provider(*provider),
            _ => Choice::Custom(servers.to_vec()),
        }
    }
}

/// The DNS of the connection in use.
#[derive(Clone, Debug, PartialEq)]
pub struct Dns {
    /// Its saved connection, and the active one.
    pub settings: OwnedObjectPath,
    pub active: OwnedObjectPath,
    pub choice: Choice,
}

/// The choices, in the order the tile goes through them: the custom servers
/// last seen are one, so going round never loses them.
pub fn choices(custom: &[String]) -> Vec<Choice> {
    let mut choices = vec![Choice::Automatic];
    choices.extend(PROVIDERS.into_iter().map(Choice::Provider));
    if !custom.is_empty() {
        choices.push(Choice::Custom(custom.to_vec()));
    }
    choices
}

/// The one after the current one.
pub fn next(current: &Choice, custom: &[String]) -> Choice {
    let choices = choices(custom);
    let index = choices.iter().position(|c| c == current);
    choices[index.map_or(0, |i| (i + 1) % choices.len())].clone()
}

/// The custom servers last used, kept for going back to them.
pub fn saved_custom() -> Vec<String> {
    Config::new(crate::APP_ID, 1)
        .and_then(|c| c.get::<Vec<String>>("custom_dns"))
        .unwrap_or_default()
}

pub fn save_custom(servers: &[String]) {
    if let Err(err) = Config::new(crate::APP_ID, 1).and_then(|c| c.set("custom_dns", servers)) {
        tracing::warn!("cannot keep the custom DNS servers: {err}");
    }
}

pub type Settings = HashMap<String, HashMap<String, OwnedValue>>;

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Connection {
    fn get_settings(&self) -> zbus::Result<Settings>;
    fn update(&self, properties: HashMap<&str, HashMap<&str, Value<'_>>>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Active {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Device {
    fn reapply(
        &self,
        connection: HashMap<&str, HashMap<&str, Value<'_>>>,
        version_id: u64,
        flags: u32,
    ) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait Manager {
    fn activate_connection(
        &self,
        connection: &ObjectPath<'_>,
        device: &ObjectPath<'_>,
        specific_object: &ObjectPath<'_>,
    ) -> zbus::Result<OwnedObjectPath>;
}

/// NetworkManager keeps IPv4 addresses as numbers, in network order.
fn ipv4_number(address: Ipv4Addr) -> u32 {
    u32::from_ne_bytes(address.octets())
}

/// The IPv4 servers in a connection's settings, and whether the network's
/// own are used too.
pub fn read(settings: &Settings) -> (bool, Vec<String>) {
    let Some(ipv4) = settings.get("ipv4") else {
        return (true, Vec::new());
    };
    let automatic = !ipv4
        .get("ignore-auto-dns")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    // Newer NetworkManager says it as text too, with IPv6 and ports.
    let data: Option<Vec<String>> = ipv4
        .get("dns-data")
        .and_then(|v| <Vec<String>>::try_from(v.try_clone().ok()?).ok());
    let servers = match data {
        Some(data) if !data.is_empty() => data,
        _ => ipv4
            .get("dns")
            .and_then(|v| <Vec<u32>>::try_from(v.try_clone().ok()?).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|n| Ipv4Addr::from(n.to_ne_bytes()).to_string())
            .collect(),
    };
    (automatic, servers)
}

pub async fn current(conn: &zbus::Connection, active: OwnedObjectPath, settings: OwnedObjectPath) -> zbus::Result<Dns> {
    let connection = ConnectionProxy::builder(conn)
        .path(settings.clone())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let (automatic, servers) = read(&connection.get_settings().await?);
    Ok(Dns {
        settings,
        active,
        choice: Choice::of(automatic, &servers),
    })
}

/// The settings to save for a choice, from the connection's own.
pub fn with_choice(settings: Settings, choice: &Choice) -> HashMap<String, HashMap<String, Value<'static>>> {
    let (ipv4, ipv6): (Vec<Ipv4Addr>, Vec<Ipv6Addr>) = match choice {
        Choice::Automatic => (Vec::new(), Vec::new()),
        Choice::Provider(p) => (
            p.ipv4.iter().filter_map(|a| a.parse().ok()).collect(),
            p.ipv6.iter().filter_map(|a| a.parse().ok()).collect(),
        ),
        Choice::Custom(servers) => {
            let parsed: Vec<IpAddr> = servers.iter().filter_map(|a| a.parse().ok()).collect();
            (
                parsed.iter().filter_map(|a| match a { IpAddr::V4(a) => Some(*a), _ => None }).collect(),
                parsed.iter().filter_map(|a| match a { IpAddr::V6(a) => Some(*a), _ => None }).collect(),
            )
        }
    };
    let by_hand = !matches!(choice, Choice::Automatic);
    let mut settings: HashMap<String, HashMap<String, Value<'static>>> = settings
        .into_iter()
        .map(|(group, values)| {
            let values = values
                .into_iter()
                .map(|(key, value)| (key, Value::from(value)))
                .collect();
            (group, values)
        })
        .collect();
    let ipv4_group = settings.entry("ipv4".to_owned()).or_default();
    let knows_data = ipv4_group.contains_key("dns-data");
    ipv4_group.insert(
        "dns".to_owned(),
        Value::from(ipv4.iter().map(|a| ipv4_number(*a)).collect::<Vec<u32>>()),
    );
    if knows_data {
        ipv4_group.insert(
            "dns-data".to_owned(),
            Value::from(ipv4.iter().map(ToString::to_string).collect::<Vec<String>>()),
        );
    }
    ipv4_group.insert("ignore-auto-dns".to_owned(), Value::from(by_hand));
    // IPv6 only where the connection uses it: else the network's servers
    // would still answer over it.
    if let Some(ipv6_group) = settings.get_mut("ipv6") {
        let method = ipv6_group
            .get("method")
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
            .unwrap_or_default();
        if !matches!(method.as_str(), "ignore" | "disabled") {
            let knows_data = ipv6_group.contains_key("dns-data");
            ipv6_group.insert(
                "dns".to_owned(),
                Value::from(ipv6.iter().map(|a| a.octets().to_vec()).collect::<Vec<Vec<u8>>>()),
            );
            if knows_data {
                ipv6_group.insert(
                    "dns-data".to_owned(),
                    Value::from(ipv6.iter().map(ToString::to_string).collect::<Vec<String>>()),
                );
            }
            ipv6_group.insert("ignore-auto-dns".to_owned(), Value::from(by_hand && !ipv6.is_empty()));
        }
    }
    settings
}

/// Saves the choice in the connection and applies it to its devices now.
pub async fn set(dns: Dns, choice: Choice) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let connection = ConnectionProxy::builder(&conn)
        .path(dns.settings.clone())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let settings = with_choice(connection.get_settings().await?, &choice);
    let borrowed: HashMap<&str, HashMap<&str, Value<'_>>> = settings
        .iter()
        .map(|(group, values)| {
            (
                group.as_str(),
                values
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.try_clone().expect("no file descriptors")))
                    .collect(),
            )
        })
        .collect();
    // Without secrets, NetworkManager keeps the ones it has.
    connection.update(borrowed).await?;
    let active = ActiveProxy::builder(&conn)
        .path(dns.active.clone())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    for device in active.devices().await.unwrap_or_default() {
        let proxy = DeviceProxy::builder(&conn).path(device.clone())?.build().await?;
        // Empty: what was just saved.
        if let Err(err) = proxy.reapply(HashMap::new(), 0, 0).await {
            tracing::warn!("cannot apply the DNS servers at once, reconnecting: {err}");
            let none = ObjectPath::from_static_str_unchecked("/");
            ManagerProxy::new(&conn)
                .await?
                .activate_connection(&dns.settings, &device, &none)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(value: Value<'static>) -> OwnedValue {
        OwnedValue::try_from(value).unwrap()
    }

    fn settings(ignore: bool, dns: Vec<u32>) -> Settings {
        let mut ipv4 = HashMap::new();
        ipv4.insert("method".to_owned(), owned(Value::from("auto")));
        ipv4.insert("ignore-auto-dns".to_owned(), owned(Value::from(ignore)));
        ipv4.insert("dns".to_owned(), owned(Value::from(dns)));
        let mut ipv6 = HashMap::new();
        ipv6.insert("method".to_owned(), owned(Value::from("auto")));
        let mut all = HashMap::new();
        all.insert("ipv4".to_owned(), ipv4);
        all.insert("ipv6".to_owned(), ipv6);
        all
    }

    #[test]
    fn reads_choices() {
        let (auto, servers) = read(&settings(false, vec![]));
        assert_eq!(Choice::of(auto, &servers), Choice::Automatic);
        let cloudflare = vec![
            ipv4_number("1.0.0.1".parse().unwrap()),
            ipv4_number("1.1.1.1".parse().unwrap()),
        ];
        let (auto, servers) = read(&settings(true, cloudflare));
        assert_eq!(Choice::of(auto, &servers), Choice::Provider(PROVIDERS[0]));
        let mine = vec![ipv4_number("192.168.0.53".parse().unwrap())];
        let (auto, servers) = read(&settings(true, mine));
        assert_eq!(Choice::of(auto, &servers), Choice::Custom(vec!["192.168.0.53".into()]));
    }

    #[test]
    fn writes_choices() {
        let written = with_choice(settings(false, vec![]), &Choice::Provider(PROVIDERS[2]));
        let owned_back: Settings = written
            .into_iter()
            .map(|(g, v)| (g, v.into_iter().map(|(k, v)| (k, owned(v))).collect()))
            .collect();
        let (auto, servers) = read(&owned_back);
        assert!(!auto);
        assert_eq!(servers, vec!["9.9.9.9".to_owned(), "149.112.112.112".to_owned()]);
        assert_eq!(
            bool::try_from(&owned_back["ipv6"]["ignore-auto-dns"]).ok(),
            Some(true)
        );
        let back = with_choice(owned_back, &Choice::Automatic);
        assert_eq!(bool::try_from(&back["ipv4"]["ignore-auto-dns"]).ok(), Some(false));
    }

    #[test]
    fn goes_round() {
        let custom = vec!["10.0.0.1".to_owned()];
        let mut choice = Choice::Automatic;
        let mut seen = Vec::new();
        for _ in 0..6 {
            choice = next(&choice, &custom);
            seen.push(choice.clone());
        }
        assert_eq!(seen[4], Choice::Custom(custom.clone()));
        assert_eq!(seen[5], Choice::Automatic);
        // Without custom servers, straight back.
        assert_eq!(next(&Choice::Provider(PROVIDERS[3]), &[]), Choice::Automatic);
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// QS_DNS_TEST=<connection name>: sets a throwaway connection's DNS
    /// through NetworkManager and back.
    #[tokio::test]
    #[ignore]
    async fn sets_a_connection() {
        let name = std::env::var("QS_DNS_TEST").unwrap();
        let conn = zbus::Connection::system().await.unwrap();
        let nm = zbus::Proxy::new(&conn, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager").await.unwrap();
        let actives: Vec<OwnedObjectPath> = nm.get_property("ActiveConnections").await.unwrap();
        for active in actives {
            let proxy = zbus::Proxy::new(&conn, "org.freedesktop.NetworkManager", active.clone(), "org.freedesktop.NetworkManager.Connection.Active").await.unwrap();
            let id: String = proxy.get_property("Id").await.unwrap();
            if id != name { continue; }
            let settings: OwnedObjectPath = proxy.get_property("Connection").await.unwrap();
            for choice in [Choice::Provider(PROVIDERS[0]), Choice::Custom(vec!["10.99.0.53".into()]), Choice::Automatic] {
                let dns = current(&conn, active.clone(), settings.clone()).await.unwrap();
                set(dns, choice.clone()).await.unwrap();
                let now = current(&conn, active.clone(), settings.clone()).await.unwrap();
                println!("{:?} -> {:?}", choice, now.choice);
                assert_eq!(now.choice, choice);
            }
            return;
        }
        panic!("no such active connection");
    }
}
