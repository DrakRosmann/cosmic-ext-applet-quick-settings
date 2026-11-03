// SPDX-License-Identifier: GPL-3.0-only

//! The quick settings that can go on the panel, and how they look.

use cosmic::{
    Element,
    iced::{self, Alignment, Background, Length, border::Radius},
    theme,
    widget::{self, button},
};

use crate::{
    app::Message,
    i18n::strings,
    widgets::{Surface, inherit_colors, readable, tile_radius},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Wifi,
    Vpn,
    Bluetooth,
    Power,
    DoNotDisturb,
    DarkMode,
    Airplane,
    Microphone,
    KeepAwake,
    Tiling,
    Magnifier,
    InvertColors,
    ScreenReader,
    HighContrast,
    Screenshot,
    Workspaces,
    Touchpad,
    HotEdge,
    Dns,
}

impl Tile {
    pub const ALL: [Tile; 19] = [
        Tile::Wifi,
        Tile::Vpn,
        Tile::Bluetooth,
        Tile::Power,
        Tile::DoNotDisturb,
        Tile::DarkMode,
        Tile::Airplane,
        Tile::Microphone,
        Tile::KeepAwake,
        Tile::Tiling,
        Tile::Magnifier,
        Tile::InvertColors,
        Tile::ScreenReader,
        Tile::HighContrast,
        Tile::Screenshot,
        Tile::Workspaces,
        Tile::Touchpad,
        Tile::HotEdge,
        Tile::Dns,
    ];

    pub const DEFAULT: [Tile; 6] = [
        Tile::Wifi,
        Tile::Bluetooth,
        Tile::Power,
        Tile::DoNotDisturb,
        Tile::DarkMode,
        Tile::Airplane,
    ];

    /// How it's saved in the configuration.
    pub fn id(self) -> &'static str {
        match self {
            Tile::Wifi => "wifi",
            Tile::Vpn => "vpn",
            Tile::Bluetooth => "bluetooth",
            Tile::Power => "power",
            Tile::DoNotDisturb => "do-not-disturb",
            Tile::DarkMode => "dark-mode",
            Tile::Airplane => "airplane",
            Tile::Microphone => "microphone",
            Tile::KeepAwake => "keep-awake",
            Tile::Tiling => "tiling",
            Tile::Magnifier => "magnifier",
            Tile::InvertColors => "invert-colors",
            Tile::ScreenReader => "screen-reader",
            Tile::HighContrast => "high-contrast",
            Tile::Screenshot => "screenshot",
            Tile::Workspaces => "workspaces",
            Tile::Touchpad => "touchpad",
            Tile::HotEdge => "hot-edge",
            Tile::Dns => "dns",
        }
    }

    pub fn from_id(id: &str) -> Option<Tile> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    /// Whether what it needs is there to put it on: Hot Edge's only with the
    /// extension.
    pub fn available(self) -> bool {
        match self {
            Tile::HotEdge => crate::services::hot_edge_installed(),
            _ => true,
        }
    }

    pub fn name(self) -> &'static str {
        let t = strings();
        match self {
            Tile::Wifi => t.wifi,
            Tile::Vpn => t.vpn,
            Tile::Bluetooth => t.bluetooth,
            Tile::Power => t.power_mode,
            Tile::DoNotDisturb => t.do_not_disturb,
            Tile::DarkMode => t.dark_style,
            Tile::Airplane => t.airplane_mode,
            Tile::Microphone => t.microphone,
            Tile::KeepAwake => t.keep_awake,
            Tile::Tiling => t.tiling,
            Tile::Magnifier => t.magnifier,
            Tile::InvertColors => t.invert_colors,
            Tile::ScreenReader => t.screen_reader,
            Tile::HighContrast => t.high_contrast,
            Tile::Screenshot => t.screenshot,
            Tile::Workspaces => t.workspaces,
            Tile::Touchpad => t.touchpad,
            Tile::HotEdge => t.hot_edge,
            Tile::Dns => t.dns,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tile::Wifi => "network-wireless-signal-good-symbolic",
            Tile::Vpn => "network-vpn-symbolic",
            Tile::Bluetooth => "bluetooth-active-symbolic",
            Tile::Power => "power-profile-balanced-symbolic",
            Tile::DoNotDisturb => "notification-disabled-symbolic",
            Tile::DarkMode => "weather-clear-night-symbolic",
            Tile::Airplane => "airplane-mode-symbolic",
            Tile::Microphone => "microphone-sensitivity-muted-symbolic",
            Tile::KeepAwake => "view-reveal-symbolic",
            Tile::Tiling => "com.system76.CosmicAppletTiling-symbolic",
            Tile::Magnifier => "zoom-in-symbolic",
            Tile::InvertColors => "preferences-color-symbolic",
            Tile::ScreenReader => "preferences-desktop-accessibility-symbolic",
            Tile::HighContrast => "color-select-symbolic",
            Tile::Screenshot => "accessories-screenshot-symbolic",
            Tile::Workspaces => "view-app-grid-symbolic",
            Tile::Touchpad => "input-touchpad-symbolic",
            Tile::HotEdge => "input-mouse-symbolic",
            Tile::Dns => "network-server-symbolic",
        }
    }
}

/// A tile on the panel, and its size: big ones show their name and take two
/// of the four columns, small ones only their icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub tile: Tile,
    pub small: bool,
}

impl Placed {
    /// `wifi`, or `wifi:small`.
    pub fn id(self) -> String {
        match self.small {
            true => format!("{}:small", self.tile.id()),
            false => self.tile.id().to_owned(),
        }
    }

    pub fn from_id(id: &str) -> Option<Placed> {
        let (id, small) = match id.strip_suffix(":small") {
            Some(id) => (id, true),
            None => (id, false),
        };
        Some(Placed {
            tile: Tile::from_id(id)?,
            small,
        })
    }

    pub fn columns(self) -> u16 {
        if self.small { 1 } else { 2 }
    }
}

/// What a tile shows and does right now.
pub struct TileState {
    pub icon: &'static str,
    pub title: &'static str,
    pub subtitle: Option<String>,
    pub on: bool,
    pub toggle: Option<Message>,
    /// The page with more of it, behind the arrow.
    pub details: Option<Message>,
}

impl TileState {
    pub fn new(
        icon: &'static str,
        title: &'static str,
        subtitle: Option<String>,
        on: bool,
        toggle: Option<Message>,
        details: Option<Message>,
    ) -> Self {
        Self {
            icon,
            title,
            subtitle,
            on,
            toggle,
            details,
        }
    }

    pub fn large<'a>(self) -> Element<'a, Message> {
        tile(self.icon, self.title, self.subtitle, self.on, self.toggle, self.details)
    }

    /// Only the icon: a click toggles, a right click opens its page; the
    /// name shows on hover.
    pub fn small<'a>(self) -> Element<'a, Message> {
        let name = match self.subtitle {
            Some(subtitle) => format!("{} · {subtitle}", self.title),
            None => self.title.to_owned(),
        };
        widget::tooltip(
            Surface::new(
                widget::container(widget::icon::icon(icon(self.icon)).size(20))
                    .center(Length::Fill)
                    .class(inherit_colors()),
                self.on,
            )
            .on_press(self.toggle)
            .on_right_press(self.details),
            widget::text::body(name),
            widget::tooltip::Position::Bottom,
        )
        .into()
    }
}

/// Lays out elements in rows of four columns, each taking its share.
pub fn grid<'a>(
    items: Vec<(Element<'a, Message>, u16)>,
    row_height: f32,
) -> Element<'a, Message> {
    const COLUMNS: u16 = 4;
    let spacing = theme::spacing().space_xs;
    let mut rows = widget::column::with_capacity(items.len()).spacing(spacing);
    let mut row: Vec<Element<'a, Message>> = Vec::new();
    let mut used = 0;
    let finish = |row: &mut Vec<Element<'a, Message>>, used: u16| {
        let mut children = std::mem::take(row);
        if used < COLUMNS {
            children.push(
                widget::space::horizontal()
                    .width(Length::FillPortion(COLUMNS - used))
                    .into(),
            );
        }
        widget::row::with_children(children)
            .spacing(spacing)
            .height(Length::Fixed(row_height))
    };
    for (element, columns) in items {
        if used + columns > COLUMNS {
            rows = rows.push(finish(&mut row, used));
            used = 0;
        }
        row.push(
            widget::container(element)
                .width(Length::FillPortion(columns))
                .height(Length::Fill)
                .into(),
        );
        used += columns;
    }
    if !row.is_empty() {
        rows = rows.push(finish(&mut row, used));
    }
    rows.into()
}

/// A round badge on a tile's corner, to take it off or put it on.
pub fn badge<'a>(icon_name: &str, message: Message) -> Element<'a, Message> {
    button::custom(
        widget::container(widget::icon::icon(icon(icon_name)).size(12))
            .center(Length::Fill)
            .class(inherit_colors()),
    )
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .class(tile_class(true))
        .on_press(message)
        .into()
}

/// A tile while editing: plain, outlined when chosen or dragged.
pub fn edit_container(selected: bool) -> theme::Container<'static> {
    theme::Container::custom(move |theme| {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        widget::container::Style {
            background: Some(Background::Color(
                if selected { component.hover } else { component.base }.into(),
            )),
            text_color: Some(component.on.into()),
            icon_color: Some(component.on.into()),
            border: cosmic::iced::Border {
                radius: tile_radius(theme).into(),
                width: if selected { 2.0 } else { 0.0 },
                color: cosmic.accent_color().into(),
            },
            ..Default::default()
        }
    })
}

/// A button like a tile while editing: plain, outlined when chosen.
pub fn edit_class(selected: bool) -> theme::Button {
    let style = move |hovered: bool, theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        button::Style {
            background: Some(Background::Color(
                if hovered { component.hover } else { component.base }.into(),
            )),
            border_radius: (tile_radius(theme) * 0.8).into(),
            border_width: if selected { 2.0 } else { 0.0 },
            border_color: cosmic.accent_color().into(),
            text_color: Some(component.on.into()),
            icon_color: Some(component.on.into()),
            ..button::Style::new()
        }
    };
    theme::Button::Custom {
        active: Box::new(move |_, theme| style(false, theme)),
        disabled: Box::new(move |theme| style(false, theme)),
        hovered: Box::new(move |_, theme| style(true, theme)),
        pressed: Box::new(move |_, theme| style(true, theme)),
    }
}

/// A symbolic icon by name. COSMIC's icon theme lacks some GNOME names, like
/// the power profiles'; those come from Adwaita's files.
pub fn icon(name: &str) -> widget::icon::Handle {
    use std::{collections::HashMap, path::PathBuf, sync::Mutex};
    static FOUND: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);

    // The system's icons, and in a Flatpak sandbox where it shows them.
    let find = |theme: &str| {
        ["/usr/share/icons", "/run/host/share/icons", "/app/share/icons"]
            .iter()
            .map(|root| PathBuf::from(root).join(theme))
            .find_map(|dir| {
                ["scalable", "symbolic"].iter().find_map(|kind| {
                    std::fs::read_dir(dir.join(kind)).ok()?.flatten().find_map(|context| {
                        let file = context.path().join(format!("{name}.svg"));
                        file.exists().then_some(file)
                    })
                })
            })
    };
    let adwaita = FOUND
        .lock()
        .unwrap()
        .get_or_insert_default()
        .entry(name.to_owned())
        .or_insert_with(|| find("Cosmic").map_or_else(|| find("Adwaita"), |_| None))
        .clone();
    match adwaita {
        Some(path) => widget::icon::from_path(path).symbolic(true),
        None => widget::icon::from_name(name.to_owned()).symbolic(true).handle(),
    }
}

/// A battery's level, as Adwaita draws it.
pub fn battery_icon(percentage: f64) -> String {
    let level = ((percentage / 10.0).round() as u32 * 10).min(100);
    format!("battery-level-{level}-symbolic")
}

/// UPower and BlueZ name full-color icons.
pub fn symbolic(name: &str) -> String {
    match name.strip_suffix("-symbolic") {
        Some(_) => name.to_owned(),
        None => format!("{name}-symbolic"),
    }
}

/// A quick setting, lit when on, maybe with a round arrow inside to more of
/// it (a right click anywhere goes there too).
pub fn tile<'a>(
    icon_name: &str,
    title: &'a str,
    subtitle: Option<String>,
    on: bool,
    toggle: Option<Message>,
    details: Option<Message>,
) -> Element<'a, Message> {
    let one_line =
        || iced::core::text::Ellipsize::End(iced::core::text::EllipsizeHeightLimit::Lines(1));
    let mut text = widget::column::with_capacity(2).push(
        widget::text::body(title)
            .font(cosmic::font::semibold())
            .ellipsize(one_line()),
    );
    if let Some(subtitle) = subtitle {
        text = text.push(widget::text::caption(subtitle).ellipsize(one_line()));
    }
    let mut content = widget::row::with_capacity(3)
        .push(widget::icon::icon(icon(icon_name)).size(20))
        .push(text.width(Length::Fill))
        .spacing(10)
        .align_y(Alignment::Center);
    if let Some(details) = &details {
        content = content.push(
            button::custom(
                widget::container(widget::icon::icon(icon("go-next-symbolic")).size(16))
                    .center(Length::Fixed(30.0))
                    .class(inherit_colors()),
            )
            .padding(0)
            .class(arrow_class(on))
            .on_press(details.clone()),
        );
    }
    let right = if details.is_some() { 8 } else { 14 };
    Surface::new(
        widget::container(content).center_y(Length::Fill).class(inherit_colors()),
        on,
    )
        .padding([0, right, 0, 14])
        .on_press(toggle)
        .on_right_press(details)
        .into()
}

/// The round arrow inside a tile: a shade of what's on the tile, over it.
fn arrow_class(on: bool) -> theme::Button {
    let style = move |hovered: bool, theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        let component = if on {
            &cosmic.accent_button
        } else {
            &cosmic.background(theme.transparent).component
        };
        let on = readable(component.on, component.base);
        let mut shade = on;
        shade.a = if hovered { 0.2 } else { 0.1 };
        button::Style {
            background: Some(Background::Color(shade)),
            icon_color: Some(on),
            border_radius: 16.0.into(),
            ..button::Style::new()
        }
    };
    theme::Button::Custom {
        active: Box::new(move |_, theme| style(false, theme)),
        disabled: Box::new(move |theme| style(false, theme)),
        hovered: Box::new(move |_, theme| style(true, theme)),
        pressed: Box::new(move |_, theme| style(true, theme)),
    }
}

/// A small choice among a few, lit when chosen.
pub fn chip_class(on: bool) -> theme::Button {
    tile_class(on)
}

fn tile_class(on: bool) -> theme::Button {
    let style = move |hovered: bool, theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        let component = if on {
            &cosmic.accent_button
        } else {
            &cosmic.background(theme.transparent).component
        };
        let radius = Radius::from(cosmic.radius_xl()[0]);
        button::Style {
            background: Some(Background::Color(
                if hovered { component.hover } else { component.base }.into(),
            )),
            border_radius: radius,
            text_color: Some(readable(component.on, component.base)),
            icon_color: Some(readable(component.on, component.base)),
            ..button::Style::new()
        }
    };
    theme::Button::Custom {
        active: Box::new(move |_, theme| style(false, theme)),
        disabled: Box::new(move |theme| style(false, theme)),
        hovered: Box::new(move |_, theme| style(true, theme)),
        pressed: Box::new(move |_, theme| style(true, theme)),
    }
}
