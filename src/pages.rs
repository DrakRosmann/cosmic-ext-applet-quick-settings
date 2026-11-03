// SPDX-License-Identifier: GPL-3.0-only

//! What the popup shows: the quick settings, a page with more of one of them,
//! or the list of quick settings to edit.

use cosmic::{
    Element,
    applet::{menu_button, padded_control},
    iced::{self, Alignment, Background, Length},
    theme,
    widget::{self, button},
};

use crate::{
    services,
    widgets::{Level, inherit_colors},
    app::{App, Dnd, Message, Page},
    bar_spacing::BarSpacing,
    media::Control,
    notifications::Notification,
    i18n::strings,
    tiles::{
        Tile, TileState, badge, battery_icon, chip_class, edit_class, edit_container, grid, icon,
        symbolic,
    },
};

/// A row of a page: icon, title, maybe a subtitle, maybe something at the end.
fn item<'a>(
    icon_name: &str,
    title: String,
    subtitle: Option<String>,
    trailing: Option<Element<'a, Message>>,
) -> widget::Button<'a, Message> {
    let mut text = widget::column::with_capacity(2).push(widget::text::body(title));
    if let Some(subtitle) = subtitle {
        text = text.push(widget::text::caption(subtitle));
    }
    let mut row = widget::row::with_capacity(4)
        .push(widget::icon::icon(icon(icon_name)).size(20))
        .push(text.width(Length::Fill))
        .spacing(theme::spacing().space_s)
        .align_y(Alignment::Center);
    if let Some(trailing) = trailing {
        row = row.push(trailing);
    }
    menu_button(row)
}

/// A time today as 15:30; another day with its date.
fn clock(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_opt(timestamp, 0).single() else {
        return String::new();
    };
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M").to_string()
    } else {
        time.format("%d/%m %H:%M").to_string()
    }
}

fn section<'a>(title: &'a str) -> Element<'a, Message> {
    padded_control(widget::text::heading(title)).into()
}

/// An app's icon by name, or a generic one.
fn app_icon(name: Option<&str>) -> widget::icon::Handle {
    match name {
        Some(path) if path.starts_with('/') => widget::icon::from_path(path.into()),
        Some(name) if !name.is_empty() => widget::icon::from_name(name.to_owned()).handle(),
        // A speaker rather than a program's icon: it's about the sound.
        _ => icon("audio-speakers-symbolic"),
    }
}

fn check<'a>() -> Element<'a, Message> {
    widget::icon::icon(icon("object-select-symbolic"))
        .size(16)
        .into()
}

fn note<'a>(text: &'a str) -> Element<'a, Message> {
    padded_control(widget::text::body(text)).into()
}

fn dimmed(theme: &cosmic::Theme) -> iced::widget::text::Style {
    let mut color = theme.cosmic().background(theme.transparent).on;
    color.alpha *= 0.65;
    iced::widget::text::Style {
        color: Some(color.into()),
        ..Default::default()
    }
}

/// A rounded card, like the tiles when off.
fn card_container<'a>(
    content: impl Into<Element<'a, Message>>,
) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(12)
        .width(Length::Fill)
        .class(theme::Container::custom(|theme| {
            let component = &theme.cosmic().background(theme.transparent).component;
            widget::container::Style {
                background: Some(Background::Color(component.base.into())),
                text_color: Some(component.on.into()),
                icon_color: Some(component.on.into()),
                border: iced::Border {
                    radius: theme.cosmic().radius_l()[0].min(CARD_RADIUS).into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
}

fn card_class() -> theme::Button {
    let style = |hovered: bool, theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        button::Style {
            background: Some(Background::Color(
                if hovered { component.hover } else { component.base }.into(),
            )),
            border_radius: cosmic.radius_l()[0].min(CARD_RADIUS).into(),
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

/// A cover's color made a card's: deep in a dark theme, pale in a light one,
/// not too loud; and what reads on it.
fn tint([r, g, b]: [u8; 3], dark: bool) -> (iced::Color, iced::Color) {
    let (r, g, b) = (f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let delta = max - min;
    let saturation = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * lightness - 1.0).abs())
    };
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } * 60.0;
    let (saturation, lightness) = if dark {
        (saturation.min(0.55), 0.24f32)
    } else {
        (saturation.min(0.5), 0.86)
    };
    // back from hue, saturation and lightness
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = c * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = lightness - c / 2.0;
    let (r, g, b) = match hue as u32 {
        0..60 => (c, x, 0.0),
        60..120 => (x, c, 0.0),
        120..180 => (0.0, c, x),
        180..240 => (0.0, x, c),
        240..300 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let background = iced::Color::from_rgb(r + m, g + m, b + m);
    let foreground = if dark {
        iced::Color::from_rgb(0.97, 0.97, 0.97)
    } else {
        iced::Color::from_rgb(0.08, 0.08, 0.08)
    };
    (background, foreground)
}

/// The progress on a tinted card, in what reads on it.
fn progress_class(color: [u8; 3]) -> theme::iced::Slider {
    use iced::widget::slider;
    let style = move |theme: &cosmic::Theme, handle: f32| {
        let (_, foreground) = tint(color, theme.cosmic().is_dark);
        let mut rest = foreground;
        rest.a = 0.25;
        slider::Style {
            rail: slider::Rail {
                backgrounds: (Background::Color(foreground), Background::Color(rest)),
                width: 4.0,
                border: iced::Border {
                    radius: 2.0.into(),
                    ..Default::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: handle },
                background: Background::Color(foreground),
                border_width: 0.0,
                border_color: iced::Color::TRANSPARENT,
            },
            breakpoint: slider::Breakpoint { color: foreground },
        }
    };
    theme::iced::Slider::Custom {
        active: std::rc::Rc::new(move |theme| style(theme, 6.0)),
        hovered: std::rc::Rc::new(move |theme| style(theme, 8.0)),
        dragging: std::rc::Rc::new(move |theme| style(theme, 8.0)),
    }
}

/// The play button on a tinted card: what reads on the card, the icon in the
/// card's color.
fn play_class(color: [u8; 3]) -> theme::Button {
    let style = move |hovered: bool, theme: &cosmic::Theme| {
        let (background, mut foreground) = tint(color, theme.cosmic().is_dark);
        if hovered {
            foreground.a = 0.85;
        }
        button::Style {
            background: Some(Background::Color(foreground)),
            border_radius: 20.0.into(),
            text_color: Some(background),
            icon_color: Some(background),
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

/// Cards' corners: rounded, never a pill, however round the theme.
const CARD_RADIUS: f32 = 16.0;
/// Between the popup's panels.
pub const PANEL_GAP: f32 = 8.0;

/// A panel of the popup, as COSMIC draws a popup: there are two on the main
/// page, the controls and the notifications, apart.
pub fn panel<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::container(content)
        .width(Length::Fill)
        .class(theme::Container::custom(|theme| {
            let cosmic = theme.cosmic();
            let background = cosmic.background(theme.transparent);
            widget::container::Style {
                background: Some(Background::Color(background.base.into())),
                text_color: Some(background.on.into()),
                icon_color: Some(background.on.into()),
                border: iced::Border {
                    radius: cosmic.corner_radii.radius_m.into(),
                    width: 1.0,
                    color: background.divider.into(),
                },
                ..Default::default()
            }
        }))
        .into()
}

/// A round button with an icon, `size` across, like the tiles when off.
fn circle_button<'a>(icon_name: &str, message: Message, size: f32) -> widget::Button<'a, Message> {
    let style = move |hovered: bool, theme: &cosmic::Theme| {
        let component = &theme.cosmic().background(theme.transparent).component;
        button::Style {
            background: Some(Background::Color(
                if hovered { component.hover } else { component.base }.into(),
            )),
            border_radius: (size / 2.0).into(),
            text_color: Some(component.on.into()),
            icon_color: Some(component.on.into()),
            ..button::Style::new()
        }
    };
    button::custom(
        widget::container(widget::icon::icon(icon(icon_name)).size(16)).center(Length::Fixed(size)),
    )
    .padding(0)
    .class(theme::Button::Custom {
        active: Box::new(move |_, theme| style(false, theme)),
        disabled: Box::new(move |theme| style(false, theme)),
        hovered: Box::new(move |_, theme| style(true, theme)),
        pressed: Box::new(move |_, theme| style(true, theme)),
    })
    .on_press(message)
}

/// A pill on a card: a shade lighter than it.
fn on_card_class() -> theme::Button {
    let style = |hovered: bool, theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        button::Style {
            background: Some(Background::Color(
                if hovered { component.pressed } else { component.hover }.into(),
            )),
            border_radius: cosmic.radius_xl().into(),
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

/// The battery pill: lighter when hovered.
fn pill_class() -> theme::Button {
    let style = |hovered: bool, theme: &cosmic::Theme| {
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        button::Style {
            background: Some(Background::Color(
                if hovered { component.hover } else { component.base }.into(),
            )),
            border_radius: cosmic.radius_xl().into(),
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

impl App {
    /// A page with a way back, maybe a switch, and a way to COSMIC Settings.
    fn page<'a>(
        &'a self,
        title: &'a str,
        switch: Option<(bool, Message)>,
        body: Vec<Element<'a, Message>>,
        more: Option<(&'a str, &'static str)>,
    ) -> Element<'a, Message> {
        let mut header = widget::row::with_capacity(4)
            .push(circle_button("go-previous-symbolic", Message::ShowPage(Page::Main), 36.0))
            .push(
                widget::text::title4(title)
                    .width(Length::Fill),
            )
            .spacing(theme::spacing().space_xs)
            .align_y(Alignment::Center);
        if let Some((on, message)) = switch {
            header = header.push(widget::toggler(on).on_toggle(move |_| message.clone()));
        }

        let mut column = widget::column::with_capacity(5)
            .push(padded_control(header))
            .push(padded_control(widget::divider::horizontal::default()))
            .push(widget::scrollable(widget::column::with_children(body)).height(Length::Shrink));
        if let Some((label, page)) = more {
            column = column
                .push(padded_control(widget::divider::horizontal::default()))
                .push(
                    menu_button(widget::text::body(label))
                        .on_press(Message::Settings(Some(page))),
                );
        }
        column.into()
    }

    pub fn wifi_page(&self) -> Element<'_, Message> {
        let t = strings();
        let mut body = Vec::new();
        match &self.wifi_networks {
            _ if !self.network.wifi_enabled => body.push(note(t.wifi_off)),
            None => body.push(note(t.searching)),
            Some(networks) => {
                for (index, network) in networks.networks.iter().enumerate() {
                    let strength = match network.strength {
                        76.. => "network-wireless-signal-excellent-symbolic",
                        51..=75 => "network-wireless-signal-good-symbolic",
                        26..=50 => "network-wireless-signal-ok-symbolic",
                        _ => "network-wireless-signal-weak-symbolic",
                    };
                    let status = if network.active {
                        t.connected
                    } else if network.saved.is_some() {
                        t.saved
                    } else if network.secure {
                        t.secured
                    } else {
                        t.open
                    };
                    let trailing = if network.active {
                        Some(check())
                    } else if network.secure {
                        Some(
                            widget::icon::icon(icon("network-wireless-encrypted-symbolic"))
                                .size(16)
                                .into(),
                        )
                    } else {
                        None
                    };
                    body.push(
                        item(strength, network.ssid.clone(), Some(status.to_owned()), trailing)
                            .on_press_maybe(
                                (!network.active).then_some(Message::JoinWifi(index)),
                            )
                            .into(),
                    );
                }
            }
        }
        self.page(
            t.wifi,
            Some((self.network.wifi_enabled, Message::ToggleWifi)),
            body,
            Some((t.more_wifi, "wireless")),
        )
    }

    pub fn vpn_page(&self) -> Element<'_, Message> {
        let t = strings();
        let mut body = Vec::new();
        if self.network.vpns.is_empty() {
            body.push(note(t.no_vpn));
        }
        for (index, vpn) in self.network.vpns.iter().enumerate() {
            let on = vpn.active.is_some();
            body.push(
                item(
                    "network-vpn-symbolic",
                    vpn.name.clone(),
                    Some(if on { t.connected } else { t.disconnected }.to_owned()),
                    on.then(check),
                )
                .on_press(Message::ConnectVpn(index))
                .into(),
            );
        }
        let on = self.network.vpns.iter().any(|vpn| vpn.active.is_some());
        self.page(
            t.vpn,
            (!self.network.vpns.is_empty()).then_some((on, Message::ToggleVpn)),
            body,
            Some((t.more_vpn, "vpn")),
        )
    }

    pub fn bluetooth_page(&self) -> Element<'_, Message> {
        let t = strings();
        let mut body = Vec::new();
        if !self.bluetooth.powered {
            body.push(note(t.bluetooth_off));
        } else if self.bluetooth.devices.is_empty() {
            body.push(note(t.no_devices));
        }
        if self.bluetooth.powered {
            for device in &self.bluetooth.devices {
                let status = if device.connected { t.connected } else { t.disconnected };
                // From BlueZ, or from UPower when the device talks its own protocol.
                let battery = device.battery.map(f64::from).or_else(|| {
                    self.peripherals
                        .iter()
                        .find(|p| p.name == device.name)
                        .map(|p| p.percentage)
                });
                let mut trailing = widget::row::with_capacity(3)
                    .spacing(theme::spacing().space_xs)
                    .align_y(Alignment::Center);
                if let Some(battery) = battery.filter(|_| device.connected) {
                    trailing = trailing
                        .push(widget::icon::icon(icon(&battery_icon(battery))).size(16))
                        .push(widget::text::caption(format!("{battery:.0} %")));
                }
                if device.connected {
                    trailing = trailing.push(check());
                }
                body.push(
                    item(
                        &symbolic(&device.icon),
                        device.name.clone(),
                        Some(status.to_owned()),
                        Some(trailing.into()),
                    )
                    .on_press(Message::ConnectBluetooth(
                        device.path.clone(),
                        !device.connected,
                    ))
                    .into(),
                );
            }
        }
        self.page(
            t.bluetooth,
            Some((self.bluetooth.powered, Message::ToggleBluetooth)),
            body,
            Some((t.more_bluetooth, "bluetooth")),
        )
    }

    pub fn power_page(&self) -> Element<'_, Message> {
        let t = strings();
        let mut body = Vec::new();
        if self.battery.is_some() || !self.peripherals.is_empty() {
            body.push(padded_control(widget::text::heading(t.batteries)).into());
            let level = |percentage: f64| -> Element<'_, Message> {
                widget::text::body(format!("{percentage:.0} %")).into()
            };
            if let Some(battery) = &self.battery {
                body.push(
                    padded_control(
                        widget::row::with_children(vec![
                            widget::icon::icon(icon(&symbolic(&battery.icon))).size(20).into(),
                            widget::text::body(t.this_computer).width(Length::Fill).into(),
                            level(battery.percentage),
                        ])
                        .spacing(theme::spacing().space_s)
                        .align_y(Alignment::Center),
                    )
                    .into(),
                );
            }
            for peripheral in &self.peripherals {
                body.push(
                    padded_control(
                        widget::row::with_children(vec![
                            widget::icon::icon(icon(peripheral.icon)).size(20).into(),
                            widget::text::body(peripheral.name.clone())
                                .width(Length::Fill)
                                .into(),
                            widget::icon::icon(icon(&battery_icon(peripheral.percentage)))
                                .size(16)
                                .into(),
                            level(peripheral.percentage),
                        ])
                        .spacing(theme::spacing().space_s)
                        .align_y(Alignment::Center),
                    )
                    .into(),
                );
            }
            if self.battery.is_some() {
                body.push(
                    padded_control(
                        widget::row::with_children(vec![
                            widget::text::body(t.show_percentage).width(Length::Fill).into(),
                            widget::toggler(self.show_percentage)
                                .on_toggle(|_| Message::ToggleShowPercentage)
                                .into(),
                        ])
                        .align_y(Alignment::Center),
                    )
                    .into(),
                );
            }
            body.push(padded_control(widget::divider::horizontal::default()).into());
        }
        if let Some(power) = &self.power {
            let profiles = [
                ("power-saver", "power-profile-power-saver-symbolic", t.power_saver, t.power_saver_about),
                ("balanced", "power-profile-balanced-symbolic", t.balanced, t.balanced_about),
                ("performance", "power-profile-performance-symbolic", t.performance, t.performance_about),
            ];
            for (id, icon_name, name, about) in profiles {
                if !power.available.iter().any(|p| p == id) {
                    continue;
                }
                let active = power.active == id;
                body.push(
                    item(icon_name, name.to_owned(), Some(about.to_owned()), active.then(check))
                        .on_press(Message::SetPowerProfile(id.to_owned()))
                        .into(),
                );
            }
        }
        self.page(t.power_mode, None, body, Some((t.more_power, "power")))
    }

    pub fn sound_page(&self) -> Element<'_, Message> {
        let t = strings();
        let spacing = theme::spacing();
        let mut body = vec![section(t.output)];
        if let Some((sinks, default)) = &self.sinks {
            for sink in sinks {
                let active = &sink.name == default;
                body.push(
                    item(sink_icon(&sink.name), sink.description.clone(), None, active.then(check))
                        .on_press(Message::SetSink(sink.name.clone()))
                        .into(),
                );
            }
        }

        // Each app's own volume.
        body.push(section(t.apps));
        match self.mixer.as_ref().map(|m| &m.streams) {
            Some(streams) if !streams.is_empty() => {
                // As in Windows' mixer: each app's name over its slider, its
                // icon inside, which mutes it.
                for stream in streams {
                    let app = stream.app.clone();
                    body.push(
                        padded_control(
                            widget::column::with_children(vec![
                                widget::text::caption(stream.app.clone()).into(),
                                Level::new(
                                    widget::icon::icon(app_icon(stream.icon.as_deref())).size(22),
                                    stream.level,
                                    move |level| Message::SetStreamVolume(app.clone(), level * 100.0),
                                )
                                .on_release(Message::AdjustDone)
                                .on_icon_press(Some(Message::ToggleStreamMute(stream.app.clone())))
                                .dimmed(stream.muted)
                                .into(),
                            ])
                            .spacing(spacing.space_xxs),
                        )
                        .into(),
                    );
                }
            }
            Some(_) => body.push(note(t.no_apps)),
            None => body.push(note(t.loading)),
        }

        // The microphone in use, and how loud it hears.
        if let Some(mixer) = &self.mixer {
            body.push(section(t.input));
            if let Some(input) = &mixer.input {
                body.push(
                    padded_control(
                        Level::new(
                            widget::icon::icon(icon(if input.muted {
                                "microphone-sensitivity-muted-symbolic"
                            } else {
                                "audio-input-microphone-symbolic"
                            }))
                            .size(20),
                            input.level,
                            |level| Message::SetInputVolume(level * 100.0),
                        )
                        .on_release(Message::AdjustDone)
                        .on_icon_press(Some(Message::ToggleInputMute))
                        .dimmed(input.muted),
                    )
                    .into(),
                );
            }
            for source in &mixer.sources {
                body.push(
                    item(
                        "audio-input-microphone-symbolic",
                        source.description.clone(),
                        None,
                        (source.name == mixer.default_source).then(check),
                    )
                    .on_press(Message::SetSource(source.name.clone()))
                    .into(),
                );
            }
        }
        self.page(t.sound, None, body, Some((t.more_sound, "sound")))
    }

    pub fn dnd_page(&self) -> Element<'_, Message> {
        let t = strings();
        let mut body = Vec::new();
        let on = self.do_not_disturb;
        let options = [
            (t.dnd_forever, Dnd::Forever, on && self.dnd_until.is_none()),
            (t.dnd_hour, Dnd::For(3600), false),
            (t.dnd_hours, Dnd::For(3 * 3600), false),
            (t.dnd_tomorrow, Dnd::Tomorrow, false),
        ];
        for (label, dnd, current) in options {
            body.push(
                item(
                    "notifications-disabled-symbolic",
                    label.to_owned(),
                    None,
                    current.then(check),
                )
                .on_press(Message::SetDoNotDisturb(dnd))
                .into(),
            );
        }
        if let Some(until) = self.dnd_until.filter(|_| on) {
            body.insert(0, note_owned(format!("{} {}", t.until, clock(until))));
        }
        self.page(
            t.do_not_disturb,
            Some((on, Message::ToggleDoNotDisturb)),
            body,
            Some((t.more_dnd, "notifications")),
        )
    }

    pub fn session_page(&self) -> Element<'_, Message> {
        let t = strings();
        let body = [
            ("system-lock-screen-symbolic", t.lock, Message::Lock),
            ("system-suspend-symbolic", t.suspend, Message::Suspend),
            ("system-reboot-symbolic", t.restart, Message::Restart),
            ("system-shutdown-symbolic", t.power_off, Message::PowerOff),
            ("system-log-out-symbolic", t.log_out, Message::LogOut),
        ]
        .into_iter()
        .map(|(icon_name, label, message)| {
            (crate::tiles::tile(icon_name, label, None, false, Some(message), None), 2)
        })
        .collect();
        let body = vec![padded_control(grid(body, 64.0)).into()];
        self.page(t.session, None, body, None)
    }

    pub fn displays_page(&self) -> Element<'_, Message> {
        let t = strings();
        let spacing = theme::spacing();
        let mut body = Vec::new();
        let Some(displays) = &self.displays else {
            body.push(note(t.loading));
            return self.page(t.displays, None, body, Some((t.more_displays, "displays")));
        };
        for (index, display) in displays.iter().enumerate() {
            if index > 0 {
                body.push(padded_control(widget::divider::horizontal::default()).into());
            }
            let title = if display.internal() {
                t.built_in.to_owned()
            } else {
                format!("{} {}", display.make, display.model).trim().to_owned()
            };
            let subtitle = if display.enabled {
                format!(
                    "{} × {} · {:.0} Hz · {}",
                    display.width,
                    display.height,
                    display.refresh as f32 / 1000.0,
                    display.name
                )
            } else {
                format!("{} · {}", t.display_off, display.name)
            };
            body.push(
                padded_control(
                    widget::row::with_children(vec![
                        widget::icon::icon(icon(if display.internal() {
                            "computer-symbolic"
                        } else {
                            "video-display-symbolic"
                        }))
                        .size(20)
                        .into(),
                        widget::column::with_children(vec![
                            widget::text::body(title).font(cosmic::font::semibold()).into(),
                            widget::text::caption(subtitle).into(),
                        ])
                        .into(),
                    ])
                    .spacing(spacing.space_s)
                    .align_y(Alignment::Center),
                )
                .into(),
            );
            if !display.enabled {
                continue;
            }

            // Brightness: COSMIC's for the built-in screen, DDC/CI for monitors.
            let brightness_icon = || widget::icon::icon(icon("display-brightness-symbolic")).size(20);
            let slider: Option<Element<'_, Message>> = if display.internal() {
                self.brightness.map(|(current, max)| {
                    Level::new(brightness_icon(), current as f32 / max.max(1) as f32, |level| {
                        Message::SetBrightness(level * 100.0)
                    })
                    .on_release(Message::AdjustDone)
                    .into()
                })
            } else {
                display.ddc.map(|(_, current, max)| {
                    Level::new(brightness_icon(), current as f32 / max.max(1) as f32, move |level| {
                        Message::SetDdcBrightness(index, level * 100.0)
                    })
                    .on_release(Message::ApplyDdcBrightness(index))
                    .into()
                })
            };
            match slider {
                Some(slider) => body.push(padded_control(slider).into()),
                None if !display.internal() => {
                    body.push(padded_control(widget::text::caption(t.no_ddc)).into())
                }
                None => {}
            }

            let chips = [1.0, 1.25, 1.5, 1.75, 2.0].map(|scale| -> Element<'_, Message> {
                button::custom(
                    widget::container(widget::text::caption(format!("{:.0}%", scale * 100.0)))
                        .center(Length::Fill)
                        .class(inherit_colors()),
                )
                .height(Length::Fixed(28.0))
                .width(Length::Fill)
                .class(chip_class((display.scale - scale).abs() < 0.01))
                .on_press(Message::SetScale(index, scale))
                .into()
            });
            body.push(
                padded_control(
                    widget::row::with_capacity(6)
                        .push(widget::text::caption(t.scale).width(Length::Fixed(56.0)))
                        .extend(chips)
                        .spacing(spacing.space_xxs)
                        .align_y(Alignment::Center),
                )
                .into(),
            );
        }
        self.page(t.displays, None, body, Some((t.more_displays, "displays")))
    }

    /// Edit the tiles in place: tap one to move or resize it, "−" takes it
    /// off, "+" puts one on.
    pub fn tiling_page(&self) -> Element<'_, Message> {
        let t = strings();
        let spacing = theme::spacing();
        let config = self.tiling_config;
        let mut body: Vec<Element<'_, Message>> = Vec::new();

        body.push(padded_control(widget::text::body(t.new_workspaces)).into());
        let choice = |label: &'static str, tile: bool| -> Element<'_, Message> {
            button::custom(
                widget::row::with_capacity(2)
                    .push_maybe((config.new_workspaces_tile == tile).then(|| {
                        widget::icon::icon(icon("object-select-symbolic")).size(16)
                    }))
                    .push(widget::text::body(label))
                    .spacing(spacing.space_xxs)
                    .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .width(Length::Fill)
            .class(chip_class(config.new_workspaces_tile == tile))
            .on_press(Message::SetNewWorkspacesTile(tile))
            .into()
        };
        body.push(
            padded_control(
                widget::row::with_children(vec![
                    choice(t.tiled, true),
                    choice(t.floating, false),
                ])
                .spacing(spacing.space_xxs),
            )
            .into(),
        );
        body.push(padded_control(widget::divider::horizontal::default()).into());

        for (action, keys) in [
            (t.navigate_windows, "Super + ←↑↓→"),
            (t.move_window, "Shift + Super + ←↑↓→"),
            (t.toggle_floating, "Super + G"),
        ] {
            body.push(
                padded_control(
                    widget::row::with_children(vec![
                        widget::text::body(action).width(Length::Fill).into(),
                        widget::text::body(keys).class(theme::Text::Custom(dimmed)).into(),
                    ])
                    .align_y(Alignment::Center),
                )
                .into(),
            );
        }
        body.push(padded_control(widget::divider::horizontal::default()).into());
        body.push(
            padded_control(
                widget::row::with_children(vec![
                    widget::text::body(t.active_hint).width(Length::Fill).into(),
                    widget::toggler(config.active_hint)
                        .on_toggle(|_| Message::ToggleActiveHint)
                        .into(),
                ])
                .align_y(Alignment::Center),
            )
            .into(),
        );

        self.page(
            t.tiling,
            self.workspace_tiling.map(|on| (on, Message::ToggleTiling)),
            body,
            Some((t.more_tiling, "window-management")),
        )
    }

    pub fn edit_page(&self) -> Element<'_, Message> {
        let t = strings();
        let spacing = theme::spacing();

        let header = widget::row::with_children(vec![
            circle_button("go-previous-symbolic", Message::ShowPage(Page::Main), 36.0).into(),
            widget::text::title4(t.edit_tiles).width(Length::Fill).into(),
            widget::tooltip(
                button::icon(icon("edit-undo-symbolic"))
                    .padding(8)
                    .class(chip_class(true))
                    .on_press(Message::ResetTiles),
                widget::text::body(t.reset),
                widget::tooltip::Position::Bottom,
            )
            .into(),
        ])
        .spacing(spacing.space_xs)
        .align_y(Alignment::Center);

        let items = self
            .tiles
            .iter()
            .enumerate()
            .map(|(index, placed)| {
                let dragged = self.drag.is_some_and(|d| d.index == index && d.moved);
                let selected = self.selected_tile == Some(index) || dragged;
                let state = self.tile_state(placed.tile);
                let icon_name = state.as_ref().map_or(placed.tile.icon(), |s| s.icon);
                let mut content = widget::row::with_capacity(2)
                    .push(widget::icon::icon(icon(icon_name)).size(20))
                    .spacing(10)
                    .align_y(Alignment::Center);
                if !placed.small {
                    content = content.push(
                        widget::text::body(placed.tile.name())
                            .font(cosmic::font::semibold())
                            .ellipsize(iced::core::text::Ellipsize::End(
                                iced::core::text::EllipsizeHeightLimit::Lines(1),
                            )),
                    );
                }
                // Press to drag it, or just click to choose it.
                let content = widget::container(content).center_y(Length::Fill);
                let content = if placed.small {
                    content.center_x(Length::Fill)
                } else {
                    content.width(Length::Fill).padding([0, 14])
                };
                let body = widget::mouse_area(content.class(edit_container(selected)))
                .on_press(Message::DragStart(index))
                .on_enter(Message::DragOver(index))
                .interaction(if self.drag.is_some() {
                    iced::mouse::Interaction::Grabbing
                } else {
                    iced::mouse::Interaction::Grab
                });
                let corner = widget::container(badge("list-remove-symbolic", Message::RemoveTile(index)))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right)
                    .align_y(iced::alignment::Vertical::Top);
                let tile: Element<'_, Message> =
                    iced::widget::stack(vec![body.into(), corner.into()]).into();
                (tile, placed.columns())
            })
            .collect();
        let shown = widget::container(grid(items, 64.0))
            .padding(spacing.space_xs)
            .class(theme::Container::custom(|theme| {
                let cosmic = theme.cosmic();
                widget::container::Style {
                    border: iced::Border {
                        radius: cosmic.radius_l().into(),
                        width: 1.0,
                        color: cosmic.accent_color().into(),
                    },
                    ..Default::default()
                }
            }));

        let mut column = widget::column::with_capacity(8)
            .push(header)
            .push(widget::text::caption(t.edit_hint).class(theme::Text::Custom(dimmed)))
            .push(shown)
            .spacing(spacing.space_s);

        // Move or resize the chosen tile.
        if let Some(placed) = self.selected_tile.and_then(|i| self.tiles.get(i).map(|p| (i, p))) {
            let (index, placed) = placed;
            let last = self.tiles.len() - 1;
            let control = |icon_name: &str, label: &'static str, message: Option<Message>| -> Element<'_, Message> {
                button::custom(
                    widget::row::with_children(vec![
                        widget::icon::icon(icon(icon_name)).size(16).into(),
                        widget::text::body(label).into(),
                    ])
                    .spacing(spacing.space_xxs)
                    .align_y(Alignment::Center),
                )
                .padding([6, 12])
                .class(chip_class(false))
                .on_press_maybe(message)
                .into()
            };
            column = column.push(
                widget::row::with_children(vec![
                    widget::text::body(placed.tile.name())
                        .font(cosmic::font::semibold())
                        .width(Length::Fill)
                        .into(),
                    control("go-previous-symbolic", t.earlier, (index > 0).then_some(Message::MoveTile(index, -1))),
                    control("go-next-symbolic", t.later, (index < last).then_some(Message::MoveTile(index, 1))),
                    if placed.small {
                        control("view-fullscreen-symbolic", t.large, Some(Message::ResizeTile(index)))
                    } else {
                        control("view-restore-symbolic", t.small, Some(Message::ResizeTile(index)))
                    },
                ])
                .spacing(spacing.space_xxs)
                .align_y(Alignment::Center),
            );
        }

        // What can go on, as round icons with "+".
        let missing: Vec<Tile> = Tile::ALL
            .into_iter()
            .filter(|tile| tile.available() && !self.tiles.iter().any(|p| p.tile == *tile))
            .collect();
        if !missing.is_empty() {
            column = column.push(widget::text::heading(t.add));
            let chips = missing
                .into_iter()
                .map(|tile| {
                    let chip = button::custom(
                        widget::container(widget::icon::icon(icon(tile.icon())).size(20))
                            .center(Length::Fill),
                    )
                    .width(Length::Fixed(52.0))
                    .height(Length::Fixed(52.0))
                    .class(edit_class(false))
                    .on_press(Message::AddTile(tile));
                    let corner = widget::container(badge("list-add-symbolic", Message::AddTile(tile)))
                        .width(Length::Fixed(60.0))
                        .height(Length::Fixed(56.0))
                        .align_x(iced::alignment::Horizontal::Right)
                        .align_y(iced::alignment::Vertical::Top);
                    let item: Element<'_, Message> = widget::column::with_children(vec![
                        iced::widget::stack(vec![
                            widget::container(chip)
                                .width(Length::Fixed(60.0))
                                .height(Length::Fixed(56.0))
                                .align_x(iced::alignment::Horizontal::Center)
                                .align_y(iced::alignment::Vertical::Bottom)
                                .into(),
                            corner.into(),
                        ])
                        .into(),
                        widget::text::caption(tile.name())
                            .align_x(iced::alignment::Horizontal::Center)
                            .width(Length::Fill)
                            .into(),
                    ])
                    .align_x(Alignment::Center)
                    .spacing(spacing.space_xxs)
                    .into();
                    (item, 1)
                })
                .collect();
            column = column.push(grid(chips, 104.0));
        }

        column = column.push(
            widget::row::with_children(vec![
                widget::text::body(t.show_active).width(Length::Fill).into(),
                widget::toggler(self.show_active)
                    .on_toggle(|_| Message::ToggleShowActive)
                    .into(),
            ])
            .padding([0, spacing.space_xxs])
            .align_y(Alignment::Center),
        );

        // Further apart on a big screen.
        let choices = BarSpacing::ALL.map(|choice| -> Element<'_, Message> {
            let chosen = self.bar_spacing == choice;
            button::custom(
                widget::row::with_capacity(2)
                    .push_maybe(chosen.then(|| {
                        widget::icon::icon(icon("object-select-symbolic")).size(16)
                    }))
                    .push(widget::text::body(choice.name()))
                    .spacing(spacing.space_xxs)
                    .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .width(Length::Fill)
            .class(chip_class(chosen))
            .on_press(Message::SetBarSpacing(choice))
            .into()
        });
        column = column.push(
            widget::column::with_children(vec![
                widget::text::body(t.bar_spacing).into(),
                widget::row::with_children(choices)
                    .spacing(spacing.space_xxs)
                    .into(),
            ])
            .spacing(spacing.space_xs)
            .padding([0, spacing.space_xxs]),
        );
        column = column.push(
            widget::row::with_children(vec![
                widget::space::horizontal().into(),
                button::custom(
                    widget::row::with_children(vec![
                        widget::icon::icon(icon("object-select-symbolic")).size(16).into(),
                        widget::text::body(t.done).into(),
                    ])
                    .spacing(spacing.space_xxs)
                    .align_y(Alignment::Center),
                )
                .padding([8, 16])
                .class(chip_class(true))
                .on_press(Message::ShowPage(Page::Main))
                .into(),
            ]),
        );

        widget::scrollable(column.padding(spacing.space_s)).height(Length::Shrink).into()
    }

    /// Two panels, apart, as Android's shade: the controls, and under them
    /// the notifications, which get what the controls leave: the controls
    /// stay put and only the notifications scroll.
    pub fn main_page(&self) -> Element<'_, Message> {
        let spacing = theme::spacing();
        let mut sections = vec![self.header()];
        sections.extend(self.privacy_banner());
        sections.extend([self.sliders(), self.tiles()]);
        sections.extend(self.media_card());
        let controls = panel(
            widget::column::with_children(sections)
                .spacing(spacing.space_s)
                .padding(spacing.space_s),
        );
        // As tall as the popup's limits let it be, from the screen's height.
        widget::column::with_children(vec![controls, panel(self.notifications_panel())])
            .spacing(PANEL_GAP)
            .into()
    }

    /// Who is using the camera or the microphone.
    fn privacy_banner(&self) -> Option<Element<'_, Message>> {
        if self.privacy.is_empty() {
            return None;
        }
        let t = strings();
        let spacing = theme::spacing();
        let row = |name: &'static str, apps: &[String], doing: &'static str, warning: bool| {
            widget::row::with_children(vec![
                widget::icon::icon(icon(name))
                    .size(16)
                    .class(theme::Svg::Custom(std::rc::Rc::new(move |theme| {
                        iced::widget::svg::Style {
                            color: Some(if warning {
                                theme.cosmic().warning_color().into()
                            } else {
                                theme.cosmic().success_color().into()
                            }),
                        }
                    })))
                    .into(),
                widget::text::body(format!("{} · {doing}", apps.join(", "))).into(),
            ])
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center)
            .into()
        };
        let mut rows: Vec<Element<'_, Message>> = Vec::new();
        if !self.privacy.camera.is_empty() {
            rows.push(row("camera-web-symbolic", &self.privacy.camera, t.using_camera, false));
        }
        if !self.privacy.microphone.is_empty() {
            rows.push(row(
                "audio-input-microphone-symbolic",
                &self.privacy.microphone,
                t.using_microphone,
                true,
            ));
        }
        Some(card_container(widget::column::with_children(rows).spacing(spacing.space_xxs)).into())
    }

    /// What's playing, with its cover, progress and controls.
    fn media_card(&self) -> Option<Element<'_, Message>> {
        let media = self.shown_media()?;
        let spacing = theme::spacing();
        let cover: Element<'_, Message> = match &media.art {
            Some(path) => widget::image(widget::image::Handle::from_path(path))
                .width(Length::Fixed(56.0))
                .height(Length::Fixed(56.0))
                .content_fit(iced::ContentFit::Cover)
                .border_radius(12.0)
                .into(),
            None => widget::container(widget::icon::icon(icon("audio-x-generic-symbolic")).size(28))
                .center(Length::Fixed(56.0))
                .class(theme::Container::custom(|theme| widget::container::Style {
                    background: Some(Background::Color(
                        theme.cosmic().background(theme.transparent).component.hover.into(),
                    )),
                    border: iced::Border {
                        radius: 12.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }))
                .into(),
        };
        let color = media.color;
        let one_line = || {
            iced::core::text::Ellipsize::End(iced::core::text::EllipsizeHeightLimit::Lines(1))
        };
        let byline = match media.artist.is_empty() {
            true => media.player.clone(),
            false => format!("{} · {}", media.artist, media.player),
        };
        let about = widget::column::with_children(vec![
            widget::text::body(media.title.clone())
                .font(cosmic::font::semibold())
                .ellipsize(one_line())
                .into(),
            widget::text::caption(byline).ellipsize(one_line()).into(),
        ])
        .width(Length::Fill);

        let length = media.length.max(1) as f32;
        let position = self.seeking.unwrap_or(media.position_now() as f32);
        let time = |micros: f32| {
            let seconds = (micros / 1_000_000.0) as u64;
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };
        let progress = widget::row::with_children(vec![
            widget::text::caption(time(position)).into(),
            {
                let slider = widget::slider(0.0..=length, position.min(length), Message::Seek)
                    .on_release(Message::SeekDone)
                    .width(Length::Fill);
                match color {
                    Some(color) => slider.class(progress_class(color)).into(),
                    None => slider.into(),
                }
            },
            widget::text::caption(time(length)).into(),
        ])
        .spacing(spacing.space_xs)
        .width(Length::Fill)
        .align_y(Alignment::Center);

        let control = |icon_name: &str, message: Option<Message>, size: u16| -> Element<'_, Message> {
            button::icon(icon(icon_name))
                .icon_size(size)
                .padding(8)
                .on_press_maybe(message)
                .into()
        };
        let play = button::custom(
            widget::container(
                widget::icon::icon(icon(if media.playing {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                }))
                .size(20),
            )
            .center(Length::Fixed(40.0))
            .class(inherit_colors()),
        )
        .class(match color {
            Some(color) => play_class(color),
            None => chip_class(true),
        })
        .on_press(Message::MediaControl(Control::PlayPause));
        // The players, when more than one: the next one, round.
        let switcher: Element<'_, Message> = if self.media.len() > 1 {
            let index = self
                .media
                .iter()
                .position(|m| m.service == media.service)
                .unwrap_or(0);
            let next = &self.media[(index + 1) % self.media.len()];
            widget::tooltip(
                button::custom(
                    widget::row::with_children(vec![
                        widget::text::caption(format!("{}/{}", index + 1, self.media.len())).into(),
                        widget::icon::icon(icon("go-next-symbolic")).size(12).into(),
                    ])
                    .spacing(2)
                    .align_y(Alignment::Center),
                )
                .padding([6, 8])
                .class(theme::Button::Text)
                .on_press(Message::SelectPlayer(next.service.clone())),
                widget::text::body(next.player.clone()),
                widget::tooltip::Position::Top,
            )
            .into()
        } else {
            widget::space::horizontal().width(Length::Fixed(0.0)).into()
        };
        let outputs = button::icon(icon("audio-speakers-symbolic"))
            .padding(6)
            .selected(self.media_outputs)
            .on_press(Message::ToggleMediaOutputs);
        // As the players in macOS and Android: what's playing and its buttons
        // in a row, the progress under them.
        let top = widget::row::with_children(vec![
            cover,
            about.into(),
            control(
                "media-skip-backward-symbolic",
                media.can_previous.then_some(Message::MediaControl(Control::Previous)),
                18,
            ),
            play.into(),
            control(
                "media-skip-forward-symbolic",
                media.can_next.then_some(Message::MediaControl(Control::Next)),
                18,
            ),
        ])
        .spacing(spacing.space_xs)
        .align_y(Alignment::Center);
        let mut bottom = widget::row::with_capacity(4)
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center);
        bottom = if media.length > 0 {
            bottom.push(progress)
        } else {
            bottom.push(widget::space::horizontal())
        };
        let bottom = bottom.push(outputs).push(switcher);
        let mut card = widget::column::with_capacity(3)
            .push(top)
            .push(bottom)
            .spacing(spacing.space_xs);
        // Where the sound goes, picked right here.
        if self.media_outputs {
            if let Some((sinks, default)) = &self.sinks {
                for sink in sinks {
                    card = card.push(
                        item(
                            sink_icon(&sink.name),
                            sink.description.clone(),
                            None,
                            (&sink.name == default).then(check),
                        )
                        .on_press(Message::SetSink(sink.name.clone())),
                    );
                }
            }
        }
        Some(match color {
            // In the cover's colors, as phones show what's playing.
            Some(color) => widget::container(card)
                .padding(14)
                .width(Length::Fill)
                .class(theme::Container::custom(move |theme| {
                    let (background, foreground) = tint(color, theme.cosmic().is_dark);
                    widget::container::Style {
                        background: Some(Background::Color(background)),
                        text_color: Some(foreground),
                        icon_color: Some(foreground),
                        border: iced::Border {
                            radius: theme.cosmic().radius_l().into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }))
                .into(),
            None => card_container(card).into(),
        })
    }

    /// The notifications received, newest first, with a way to clear them;
    /// or that there are none.
    fn notifications_panel(&self) -> Element<'_, Message> {
        let t = strings();
        let spacing = theme::spacing();
        if self.notifications.is_empty() {
            // As Android says it, in a line: little room may be left.
            return widget::container(
                widget::row::with_children(vec![
                    widget::icon::icon(icon("notification-symbolic")).size(16).into(),
                    widget::text::body(t.all_caught_up)
                        .class(theme::Text::Custom(dimmed))
                        .into(),
                ])
                .spacing(spacing.space_xs)
                .align_y(Alignment::Center),
            )
            .center_x(Length::Fill)
            .padding(spacing.space_s)
            .into();
        }
        let header = widget::row::with_children(vec![
            widget::text::heading(t.notifications).width(Length::Fill).into(),
            button::custom(
                widget::row::with_children(vec![
                    widget::icon::icon(icon("edit-clear-all-symbolic")).size(16).into(),
                    widget::text::caption(t.clear_all).into(),
                ])
                .spacing(spacing.space_xxs)
                .align_y(Alignment::Center),
            )
            .padding([6, 12])
            .class(pill_class())
            .on_press(Message::ClearNotifications)
            .into(),
        ])
        .padding([0, 0, 0, spacing.space_xxs])
        .align_y(Alignment::Center);

        // An app's notifications together, where its newest is; only that one,
        // with how many more, until asked for the rest.
        let mut groups: Vec<(&str, Vec<&Notification>)> = Vec::new();
        for notification in &self.notifications {
            match groups.iter_mut().find(|(app, _)| *app == notification.app) {
                Some((_, group)) => group.push(notification),
                None => groups.push((&notification.app, vec![notification])),
            }
        }
        let mut cards: Vec<Element<'_, Message>> = Vec::new();
        for (app, group) in groups {
            let expanded = group.len() > 1 && self.expanded.contains(app);
            if expanded {
                for notification in &group {
                    cards.push(self.notification_card(notification, None));
                }
                cards.push(
                    button::custom(
                        widget::row::with_children(vec![
                            widget::text::caption(t.show_less).into(),
                            widget::icon::icon(icon("go-up-symbolic")).size(12).into(),
                        ])
                        .spacing(spacing.space_xxs)
                        .align_y(Alignment::Center),
                    )
                    .padding([4, 12])
                    .class(pill_class())
                    .on_press(Message::ToggleGroup(app.to_owned()))
                    .into(),
                );
            } else {
                let more = (group.len() > 1).then(|| (group.len() - 1, app.to_owned()));
                cards.push(self.notification_card(group[0], more));
            }
        }
        let list = widget::scrollable(widget::column::with_children(cards).spacing(spacing.space_xs))
            .height(Length::Shrink);
        widget::column::with_children(vec![header.into(), list.into()])
            .spacing(spacing.space_xs)
            .padding(spacing.space_s)
            .into()
    }

    /// A notification as a card; `more`, how many more its app has, and the
    /// app, behind a count that shows them.
    fn notification_card<'a>(
        &'a self,
        notification: &'a Notification,
        more: Option<(usize, String)>,
    ) -> Element<'a, Message> {
        let t = strings();
        let spacing = theme::spacing();
        let app_icon = if notification.icon.starts_with('/') {
            widget::icon::from_path(notification.icon.clone().into())
        } else if notification.icon.is_empty() {
            icon("preferences-system-notifications-symbolic")
        } else {
            widget::icon::from_name(notification.icon.clone()).handle()
        };
        let elapsed = notification.time.elapsed().unwrap_or_default().as_secs();
        let ago = match elapsed {
            0..60 => t.now.to_owned(),
            60..3600 => format!("{} min", elapsed / 60),
            3600..86400 => format!("{} h", elapsed / 3600),
            _ => format!("{} d", elapsed / 86400),
        };
        let source_text = if notification.app.is_empty() {
            ago
        } else {
            format!("{} · {ago}", notification.app)
        };
        let mut source = widget::row::with_capacity(3)
            .push(widget::text::caption(source_text).class(theme::Text::Custom(dimmed)))
            .spacing(spacing.space_xxs)
            .align_y(Alignment::Center);
        // New since the popup was last open: an accent dot.
        if self.is_new(notification) {
            source = source.push(
                widget::container(widget::space::horizontal().width(Length::Fixed(0.0)))
                    .width(Length::Fixed(8.0))
                    .height(Length::Fixed(8.0))
                    .class(theme::Container::custom(|theme| widget::container::Style {
                        background: Some(Background::Color(theme.cosmic().accent_color().into())),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })),
            );
        }
        let one_line =
            || iced::core::text::Ellipsize::End(iced::core::text::EllipsizeHeightLimit::Lines(1));
        let mut text = widget::column::with_capacity(4)
            .push(source)
            .push(
                widget::text::body(notification.summary.clone())
                    .font(cosmic::font::semibold())
                    .ellipsize(one_line()),
            );
        if !notification.body.is_empty() {
            text = text.push(widget::text::caption(notification.body.clone()).ellipsize(
                iced::core::text::Ellipsize::End(iced::core::text::EllipsizeHeightLimit::Lines(3)),
            ));
        }
        // Its buttons, like Reply or Mark as read; the default is the click.
        let actions: Vec<Element<'_, Message>> = notification
            .actions
            .iter()
            .filter(|(key, label)| key != "default" && !label.is_empty())
            .map(|(key, label)| {
                button::custom(widget::text::caption(label.clone()))
                    .padding([4, 12])
                    .class(on_card_class())
                    .on_press(Message::NotificationAction(notification.id, key.clone()))
                    .into()
            })
            .collect();
        if !actions.is_empty() {
            text = text.push(
                widget::row::with_children(actions)
                    .spacing(spacing.space_xxs)
                    .padding([spacing.space_xxs, 0, 0, 0]),
            );
        }
        let key = (notification.id, notification.restored);
        // On the right: dismiss, and under it how many more of the app's.
        let mut side = widget::column::with_capacity(2)
            .push(
                button::icon(icon("window-close-symbolic"))
                    .padding(4)
                    .on_press(Message::DismissNotification(key)),
            )
            .spacing(spacing.space_xxs)
            .align_x(Alignment::End);
        if let Some((count, app)) = more {
            side = side.push(
                button::custom(
                    widget::row::with_children(vec![
                        widget::text::caption(format!("+{count}")).into(),
                        widget::icon::icon(icon("go-down-symbolic")).size(12).into(),
                    ])
                    .spacing(2)
                    .align_y(Alignment::Center),
                )
                .padding([2, 8])
                .class(on_card_class())
                .on_press(Message::ToggleGroup(app)),
            );
        }
        let content = widget::row::with_children(vec![
            widget::icon::icon(app_icon).size(32).into(),
            text.width(Length::Fill).into(),
            side.into(),
        ])
        .spacing(spacing.space_s);
        button::custom(content)
            .padding(12)
            .width(Length::Fill)
            .class(card_class())
            .on_press(Message::OpenNotification(key))
            .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let t = strings();
        let mut row = widget::row::with_capacity(6)
            .spacing(theme::spacing().space_xs)
            .align_y(Alignment::Center);
        if let Some(battery) = &self.battery {
            // A way to the batteries and power modes, with how long it lasts.
            let mut label = format!("{:.0} %", battery.percentage);
            if let Some(left) = battery.time_left {
                label.push_str(&format!(" · {}", crate::app::duration(left)));
            }
            row = row.push(
                button::custom(
                    widget::row::with_children(vec![
                        widget::icon::icon(icon(&symbolic(&battery.icon))).size(16).into(),
                        widget::text::body(label).into(),
                    ])
                    .spacing(theme::spacing().space_xxs)
                    .align_y(Alignment::Center),
                )
                .padding([10, 14])
                .class(pill_class())
                .on_press(Message::ShowPage(Page::Power)),
            );
        }
        row = row.push(widget::space::horizontal());
        let action = |icon_name: &'static str, tooltip: &'static str, message| {
            widget::tooltip(
                circle_button(icon_name, message, 36.0),
                widget::text::body(tooltip),
                widget::tooltip::Position::Bottom,
            )
        };
        // Close together, to leave the battery room for a long time left.
        let actions = widget::row::with_capacity(5)
            .spacing(theme::spacing().space_xxs)
            .push(action("document-edit-symbolic", t.edit, Message::ShowPage(Page::Edit)))
            .push(action("accessories-screenshot-symbolic", t.screenshot, Message::Screenshot))
            .push(action("emblem-system-symbolic", t.settings, Message::Settings(None)))
            .push(action("system-lock-screen-symbolic", t.lock, Message::Lock))
            .push(action("system-shutdown-symbolic", t.session, Message::ShowPage(Page::Session)));
        row.push(actions).into()
    }

    /// Volume and brightness, thick, as phones have them, each with a round
    /// arrow to its page.
    fn sliders(&self) -> Element<'_, Message> {
        let spacing = theme::spacing();
        let mut column = widget::column::with_capacity(2).spacing(spacing.space_xs);
        let slider_row = |level: Level<'static, Message>, more: Message| {
            widget::row::with_children(vec![
                level.into(),
                circle_button("go-next-symbolic", more, 44.0).into(),
            ])
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center)
        };
        if let Some(volume) = &self.volume {
            column = column.push(slider_row(
                // the icon mutes
                Level::new(
                    widget::icon::icon(icon(self.volume_icon())).size(20),
                    volume.level,
                    |level| Message::SetVolume(level * 100.0),
                )
                .on_release(Message::AdjustDone)
                .on_icon_press(Some(Message::ToggleMute))
                .dimmed(volume.muted),
                Message::ShowPage(Page::Sound),
            ));
        }
        if let Some((current, max)) = self.brightness {
            column = column.push(slider_row(
                Level::new(
                    widget::icon::icon(icon("display-brightness-symbolic")).size(20),
                    current as f32 / max.max(1) as f32,
                    |level| Message::SetBrightness(level * 100.0),
                )
                .on_release(Message::AdjustDone),
                Message::ShowPage(Page::Displays),
            ));
        }
        column.into()
    }

    /// The tiles that are on, for the bar: icon and what it says. Wi-Fi has
    /// its own.
    pub(crate) fn active_tiles(&self) -> Vec<(&'static str, String)> {
        Tile::ALL
            .into_iter()
            .filter(|tile| *tile != Tile::Wifi)
            .filter_map(|tile| self.tile_state(tile))
            .filter(|state| state.on)
            .map(|state| {
                let text = match &state.subtitle {
                    Some(subtitle) => format!("{} · {subtitle}", state.title),
                    None => state.title.to_owned(),
                };
                (state.icon, text)
            })
            .collect()
    }

    fn tile_state(&self, tile: Tile) -> Option<TileState> {
        let t = strings();
        Some(match tile {
            Tile::Wifi => {
                let wired =
                    matches!(&self.network.connection, Some((_, k)) if k == "802-3-ethernet");
                let subtitle = match &self.network.connection {
                    Some((name, _)) => name.clone(),
                    None if self.network.wifi_enabled => t.not_connected.to_owned(),
                    None => t.off.to_owned(),
                };
                TileState::new(
                    self.network_icon(),
                    if wired { t.network } else { t.wifi },
                    Some(subtitle),
                    self.network.wifi_enabled || wired,
                    Some(Message::ToggleWifi),
                    Some(Message::ShowPage(Page::Wifi)),
                )
            }
            Tile::Vpn => {
                let on: Vec<&str> = self
                    .network
                    .vpns
                    .iter()
                    .filter(|vpn| vpn.active.is_some())
                    .map(|vpn| vpn.name.as_str())
                    .collect();
                let subtitle = match on.as_slice() {
                    [] if self.network.vpns.is_empty() => t.no_vpn.to_owned(),
                    [] => t.off.to_owned(),
                    names => names.join(", "),
                };
                TileState::new(
                    if on.is_empty() {
                        "network-vpn-disabled-symbolic"
                    } else {
                        "network-vpn-symbolic"
                    },
                    t.vpn,
                    Some(subtitle),
                    !on.is_empty(),
                    Some(Message::ToggleVpn),
                    Some(Message::ShowPage(Page::Vpn)),
                )
            }
            Tile::Bluetooth => {
                self.bluetooth.adapter.as_ref()?;
                let subtitle = match self.bluetooth.connected.as_slice() {
                    [] if self.bluetooth.powered => t.on.to_owned(),
                    [] => t.off.to_owned(),
                    // with its battery, as phones show it
                    [one] => {
                        let battery = self
                            .bluetooth
                            .devices
                            .iter()
                            .find(|d| d.connected && &d.name == one)
                            .and_then(|d| d.battery)
                            .map(f64::from)
                            .or_else(|| {
                                self.peripherals
                                    .iter()
                                    .find(|p| &p.name == one)
                                    .map(|p| p.percentage)
                            });
                        match battery {
                            Some(battery) => format!("{one} · {battery:.0} %"),
                            None => one.clone(),
                        }
                    }
                    many => format!("{} {}", many.len(), t.devices),
                };
                TileState::new(
                    if self.bluetooth.powered {
                        "bluetooth-active-symbolic"
                    } else {
                        "bluetooth-disabled-symbolic"
                    },
                    t.bluetooth,
                    Some(subtitle),
                    self.bluetooth.powered,
                    Some(Message::ToggleBluetooth),
                    Some(Message::ShowPage(Page::Bluetooth)),
                )
            }
            Tile::Power => {
                let power = self.power.as_ref().filter(|p| p.available.len() > 1)?;
                let (icon_name, name) = match power.active.as_str() {
                    "power-saver" => ("power-profile-power-saver-symbolic", t.power_saver),
                    "performance" => ("power-profile-performance-symbolic", t.performance),
                    _ => ("power-profile-balanced-symbolic", t.balanced),
                };
                TileState::new(
                    icon_name,
                    t.power_mode,
                    Some(name.to_owned()),
                    power.active != "balanced",
                    Some(Message::NextPowerProfile),
                    Some(Message::ShowPage(Page::Power)),
                )
            }
            Tile::DoNotDisturb => TileState::new(
                tile.icon(),
                t.do_not_disturb,
                self.dnd_until
                    .filter(|_| self.do_not_disturb)
                    .map(|until| format!("{} {}", t.until, clock(until))),
                self.do_not_disturb,
                Some(Message::ToggleDoNotDisturb),
                Some(Message::ShowPage(Page::DoNotDisturb)),
            ),
            Tile::DarkMode => TileState::new(
                tile.icon(),
                t.dark_style,
                None,
                self.dark_mode,
                Some(Message::ToggleDarkMode),
                None,
            ),
            Tile::Airplane => TileState::new(
                tile.icon(),
                t.airplane_mode,
                None,
                self.airplane_mode(),
                Some(Message::ToggleAirplaneMode),
                None,
            ),
            Tile::Microphone => {
                let muted = self.microphone?;
                TileState::new(
                    if muted {
                        "microphone-sensitivity-muted-symbolic"
                    } else {
                        "audio-input-microphone-symbolic"
                    },
                    t.microphone,
                    None,
                    muted,
                    Some(Message::ToggleMicrophone),
                    None,
                )
            }
            Tile::KeepAwake => TileState::new(
                tile.icon(),
                t.keep_awake,
                None,
                self.inhibit.is_some(),
                Some(Message::ToggleKeepAwake),
                None,
            ),
            Tile::Tiling => TileState::new(
                tile.icon(),
                t.tiling,
                None,
                self.workspace_tiling?,
                Some(Message::ToggleTiling),
                Some(Message::ShowPage(Page::Tiling)),
            ),
            Tile::Magnifier => TileState::new(
                tile.icon(),
                t.magnifier,
                None,
                self.magnifier?,
                Some(Message::ToggleMagnifier),
                None,
            ),
            Tile::InvertColors => TileState::new(
                tile.icon(),
                t.invert_colors,
                None,
                self.screen_filter?.0,
                Some(Message::ToggleInvertColors),
                None,
            ),
            Tile::ScreenReader => TileState::new(
                tile.icon(),
                t.screen_reader,
                None,
                self.screen_reader?,
                Some(Message::ToggleScreenReader),
                None,
            ),
            Tile::HighContrast => TileState::new(
                tile.icon(),
                t.high_contrast,
                None,
                self.high_contrast(),
                Some(Message::ToggleHighContrast),
                None,
            ),
            // Shortcuts: never lit.
            Tile::Screenshot => TileState::new(
                tile.icon(),
                t.screenshot,
                None,
                false,
                Some(Message::Screenshot),
                None,
            ),
            Tile::Workspaces => TileState::new(
                tile.icon(),
                t.workspaces,
                None,
                false,
                Some(Message::OpenWorkspaces),
                None,
            ),
            Tile::HotEdge => {
                if !tile.available() {
                    return None;
                }
                TileState::new(
                    tile.icon(),
                    t.hot_edge,
                    None,
                    self.hot_edge,
                    Some(Message::ToggleHotEdge),
                    Some(Message::OpenHotEdge),
                )
            }
            Tile::Touchpad => {
                let touchpad = self.touchpad?;
                TileState::new(
                    if touchpad == services::Touchpad::Disabled {
                        "touchpad-disabled-symbolic"
                    } else {
                        tile.icon()
                    },
                    t.touchpad,
                    (touchpad == services::Touchpad::DisabledOnExternalMouse)
                        .then(|| t.off_with_mouse.to_owned()),
                    touchpad != services::Touchpad::Disabled,
                    Some(Message::ToggleTouchpad),
                    None,
                )
            }
        })
    }

    fn tiles(&self) -> Element<'_, Message> {
        let items = self
            .tiles
            .iter()
            .filter_map(|placed| {
                let state = self.tile_state(placed.tile)?;
                let element = if placed.small { state.small() } else { state.large() };
                Some((element, placed.columns()))
            })
            .collect();
        grid(items, 64.0)
    }
}

fn note_owned<'a>(text: String) -> Element<'a, Message> {
    padded_control(widget::text::body(text)).into()
}

fn sink_icon(name: &str) -> &'static str {
    if name.contains("HDMI") || name.contains("hdmi") {
        "video-display-symbolic"
    } else if name.contains("bluez") {
        "audio-headphones-symbolic"
    } else {
        "audio-speakers-symbolic"
    }
}
