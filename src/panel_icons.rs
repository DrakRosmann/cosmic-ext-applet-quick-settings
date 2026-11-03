// SPDX-License-Identifier: GPL-3.0-only

//! Which icons the panel shows, and how big: a mouse that always says 100%
//! can go, the network can stay with the official applet, and so on.

use std::collections::BTreeSet;

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};

use crate::i18n::strings;

/// An icon of the panel that can be hidden.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PanelIcon {
    Notifications,
    Network,
    Volume,
    /// The computer's own battery.
    Battery,
    /// A wireless mouse, keyboard, headset…, by its name.
    Peripheral(String),
}

impl PanelIcon {
    /// How it's saved in the configuration.
    pub fn id(&self) -> String {
        match self {
            PanelIcon::Notifications => "notifications".to_owned(),
            PanelIcon::Network => "network".to_owned(),
            PanelIcon::Volume => "volume".to_owned(),
            PanelIcon::Battery => "battery".to_owned(),
            PanelIcon::Peripheral(name) => format!("peripheral:{name}"),
        }
    }

    pub fn from_id(id: &str) -> Option<PanelIcon> {
        Some(match id {
            "notifications" => PanelIcon::Notifications,
            "network" => PanelIcon::Network,
            "volume" => PanelIcon::Volume,
            "battery" => PanelIcon::Battery,
            _ => PanelIcon::Peripheral(id.strip_prefix("peripheral:")?.to_owned()),
        })
    }

    pub fn name(&self) -> String {
        let t = strings();
        match self {
            PanelIcon::Notifications => t.notifications.to_owned(),
            PanelIcon::Network => t.network.to_owned(),
            PanelIcon::Volume => t.volume.to_owned(),
            PanelIcon::Battery => t.this_computer.to_owned(),
            PanelIcon::Peripheral(name) => name.clone(),
        }
    }
}

/// The icons left out, kept by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hidden(BTreeSet<String>);

impl Hidden {
    pub fn shows(&self, icon: &PanelIcon) -> bool {
        !self.0.contains(&icon.id())
    }

    pub fn toggle(&mut self, icon: &PanelIcon) {
        let id = icon.id();
        if !self.0.remove(&id) {
            self.0.insert(id);
        }
    }

    pub fn load() -> Hidden {
        let ids = Config::new(crate::APP_ID, 1)
            .and_then(|c| c.get::<Vec<String>>("hidden_panel_icons"))
            .unwrap_or_default();
        Hidden(
            ids.into_iter()
                .filter(|id| PanelIcon::from_id(id).is_some())
                .collect(),
        )
    }

    pub fn save(&self) {
        let ids: Vec<&String> = self.0.iter().collect();
        if let Err(err) =
            Config::new(crate::APP_ID, 1).and_then(|c| c.set("hidden_panel_icons", ids))
        {
            tracing::warn!("cannot save the panel's icons: {err}");
        }
    }
}

/// How big the panel's icons are, next to the size the panel suggests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl IconSize {
    pub const ALL: [IconSize; 3] = [IconSize::Small, IconSize::Medium, IconSize::Large];

    pub fn id(self) -> &'static str {
        match self {
            IconSize::Small => "small",
            IconSize::Medium => "medium",
            IconSize::Large => "large",
        }
    }

    pub fn name(self) -> &'static str {
        let t = strings();
        match self {
            IconSize::Small => t.spacing_small,
            IconSize::Medium => t.spacing_medium,
            IconSize::Large => t.spacing_large,
        }
    }

    /// The icons' size, from the panel's.
    pub fn apply(self, suggested: u16) -> u16 {
        match self {
            IconSize::Small => (suggested * 4 / 5).max(12),
            IconSize::Medium => suggested,
            IconSize::Large => suggested + suggested / 4,
        }
    }

    pub fn load() -> IconSize {
        Config::new(crate::APP_ID, 1)
            .and_then(|c| c.get::<String>("panel_icon_size"))
            .ok()
            .and_then(|id| Self::ALL.into_iter().find(|s| s.id() == id))
            .unwrap_or_default()
    }

    pub fn save(self) {
        if let Err(err) =
            Config::new(crate::APP_ID, 1).and_then(|c| c.set("panel_icon_size", self.id()))
        {
            tracing::warn!("cannot save the panel's icon size: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        for icon in [
            PanelIcon::Notifications,
            PanelIcon::Network,
            PanelIcon::Volume,
            PanelIcon::Battery,
            PanelIcon::Peripheral("MX Master 3".into()),
        ] {
            assert_eq!(PanelIcon::from_id(&icon.id()), Some(icon));
        }
        assert_eq!(PanelIcon::from_id("nonsense"), None);
    }

    #[test]
    fn hiding() {
        let mut hidden = Hidden::default();
        let mouse = PanelIcon::Peripheral("Mouse".into());
        assert!(hidden.shows(&mouse));
        hidden.toggle(&mouse);
        assert!(!hidden.shows(&mouse));
        assert!(hidden.shows(&PanelIcon::Battery));
        hidden.toggle(&mouse);
        assert!(hidden.shows(&mouse));
    }
}
