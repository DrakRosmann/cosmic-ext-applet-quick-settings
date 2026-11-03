// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use cosmic::{
    Element, Task, app,
    iced::{
        self, Alignment, Limits, Subscription, event,
        platform_specific::shell::commands::popup::destroy_popup, window,
    },
    theme,
    widget::{self, button},
};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    pages,
    audio::{self, Mixer, Privacy},
    bar_spacing::BarSpacing,
    i18n::strings,
    panel_icons::{Hidden, IconSize, PanelIcon},
    workspaces,
    media::{self, Control, Media},
    notifications::{self, Notification},
    services::{
        self, Battery, Bluetooth, DARK_MODE, DO_NOT_DISTURB, Joined, Network, PowerProfiles,
        HIDE_ACTIVE, SHOW_PERCENTAGE,
        Display, Peripheral, Sink, Update, Volume, WifiNetworks,
    },
    tiles::{Placed, Tile, icon, symbolic},
    transition::Transition,
};

const WIDTH: f32 = 420.0;
/// The popup grows with what it shows up to this, until the screen's height
/// is known; past it the notifications scroll.
pub const MAX_HEIGHT: f32 = 800.0;
/// Of the screen, left out of the popup's height: the panel and some air.
const SCREEN_MARGIN: f32 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Edit,
    Wifi,
    Vpn,
    Bluetooth,
    Power,
    Sound,
    Displays,
    Tiling,
    DoNotDisturb,
    Session,
    Dns,
}

impl Page {
    pub fn from_name(name: &str) -> Option<Page> {
        Some(match name {
            "main" => Page::Main,
            "edit" => Page::Edit,
            "wifi" => Page::Wifi,
            "vpn" => Page::Vpn,
            "bluetooth" => Page::Bluetooth,
            "power" => Page::Power,
            "sound" => Page::Sound,
            "displays" => Page::Displays,
            "tiling" => Page::Tiling,
            "dnd" => Page::DoNotDisturb,
            "session" => Page::Session,
            "dns" => Page::Dns,
            _ => return None,
        })
    }
}

/// How long do not disturb lasts.
#[derive(Clone, Copy, Debug)]
pub enum Dnd {
    Off,
    Forever,
    /// Seconds.
    For(i64),
    /// Until 8 tomorrow morning.
    Tomorrow,
}

/// A notification: its id, and whether it is kept from an earlier run, whose
/// ids may be reused.
pub type NotificationKey = (u32, bool);

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    /// Opens the popup on a page by itself, for screenshots.
    Preview(Page),
    Closed(window::Id),
    ShowPage(Page),
    Update(Update),
    SetVolume(f32),
    ToggleMute,
    SetBrightness(f32),
    ToggleWifi,
    ToggleBluetooth,
    NextPowerProfile,
    SetPowerProfile(String),
    ToggleDoNotDisturb,
    ToggleDarkMode,
    ToggleAirplaneMode,
    ToggleMicrophone,
    ToggleKeepAwake,
    ToggleTouchpad,
    ToggleHotEdge,
    /// Hot Edge's own settings app.
    OpenHotEdge,
    /// A screen's name and height.
    Output(Option<String>, Option<i32>),
    /// A window's new size: the panel button's, for the popup to line up with.
    Resized(window::Id, iced::Size),
    ToggleShowPercentage,
    ToggleShowActive,
    SettingsChanged,
    Inhibited(Option<u32>),
    WifiNetworks(Option<WifiNetworks>),
    /// Join the network at this index of the list.
    JoinWifi(usize),
    ConnectBluetooth(OwnedObjectPath, bool),
    /// The last VPN used on, or those on off.
    ToggleVpn,
    /// A VPN, by index, on or off.
    ConnectVpn(usize),
    /// The DNS servers: the next choice, or this one.
    NextDns,
    SetDns(crate::dns::Choice),
    Sinks(Option<(Vec<Sink>, String)>),
    SetSink(String),
    Displays(Vec<Display>),
    Workspaces(workspaces::Event),
    ToggleTiling,
    SetNewWorkspacesTile(bool),
    ToggleActiveHint,
    A11y(services::A11y),
    ScreenReader(cosmic_settings_accessibility_subscription::Response),
    ToggleMagnifier,
    ToggleInvertColors,
    ToggleScreenReader,
    ToggleHighContrast,
    OpenWorkspaces,
    Media(Vec<Media>),
    SelectPlayer(String),
    /// The audio outputs, in the media card.
    ToggleMediaOutputs,
    MediaControl(Control),
    /// Dragging the progress bar, in microseconds, then letting go.
    Seek(f32),
    SeekDone,
    /// Every second while something plays, for the progress bar.
    Tick,
    Notification(notifications::Event),
    OpenNotification(NotificationKey),
    DismissNotification(NotificationKey),
    NotificationAction(u32, String),
    ClearNotifications,
    /// Show all of an app's notifications, or its newest only.
    ToggleGroup(String),
    SetDoNotDisturb(Dnd),
    CheckDoNotDisturb,
    Mixer(Mixer),
    /// An app, by name in the mixer, and its volume in percent.
    SetStreamVolume(String, f32),
    ToggleStreamMute(String),
    /// A volume slider let go.
    AdjustDone,
    SetSource(String),
    SetInputVolume(f32),
    ToggleInputMute,
    Privacy(Privacy),
    /// Scrolling over the volume or battery icon, in steps.
    ScrollVolume(f32),
    ScrollBrightness(f32),
    Surface(cosmic::surface::Action),
    Suspend,
    Restart,
    LogOut,
    /// A display, by index, and its new scale.
    SetScale(usize, f32),
    /// A display's brightness over DDC while dragging, then when let go.
    SetDdcBrightness(usize, f32),
    ApplyDdcBrightness(usize),
    /// A tile pressed while editing; dragged over another; let go.
    DragStart(usize),
    DragOver(usize),
    DragEnd,
    MoveTile(usize, isize),
    ResizeTile(usize),
    RemoveTile(usize),
    AddTile(Tile),
    ResetTiles,
    /// How far apart the icons on the panel are.
    SetBarSpacing(BarSpacing),
    /// An icon on the panel shown or hidden.
    TogglePanelIcon(PanelIcon),
    SetIconSize(IconSize),
    /// A page of COSMIC Settings, or all of it.
    Settings(Option<&'static str>),
    Screenshot,
    Lock,
    PowerOff,
    None,
}

/// A tile being dragged to a new place, and whether it moved at all: one
/// that didn't was only clicked.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub index: usize,
    pub moved: bool,
}

#[derive(Default)]
pub struct App {
    core: app::Core,
    popup: Option<window::Id>,
    page: Option<Page>,
    pub(crate) tiles: Vec<Placed>,
    /// The tile being moved or resized while editing.
    pub(crate) selected_tile: Option<usize>,
    pub(crate) drag: Option<Drag>,
    pub(crate) network: Network,
    pub(crate) wifi_networks: Option<WifiNetworks>,
    pub(crate) bluetooth: Bluetooth,
    pub(crate) battery: Option<Battery>,
    pub(crate) peripherals: Vec<Peripheral>,
    pub(crate) volume: Option<Volume>,
    pub(crate) microphone: Option<bool>,
    pub(crate) sinks: Option<(Vec<Sink>, String)>,
    pub(crate) displays: Option<Vec<Display>>,
    pub(crate) workspace_tiling: Option<bool>,
    workspaces_tx: Option<cosmic::cctk::sctk::reexports::calloop::channel::Sender<bool>>,
    pub(crate) tiling_config: services::TilingConfig,
    a11y_tx: Option<cosmic::cctk::sctk::reexports::calloop::channel::Sender<services::AccessibilityRequest>>,
    pub(crate) magnifier: Option<bool>,
    /// Inverted colors, and the color filter on top, kept as they are.
    pub(crate) screen_filter: Option<(bool, Option<services::ColorFilter>)>,
    pub(crate) screen_reader: Option<bool>,
    screen_reader_tx: Option<tokio::sync::mpsc::UnboundedSender<cosmic_settings_accessibility_subscription::Request>>,
    /// Playing first.
    pub(crate) media: Vec<Media>,
    /// The player picked, by service.
    pub(crate) player: Option<String>,
    pub(crate) media_outputs: bool,
    pub(crate) mixer: Option<Mixer>,
    pub(crate) privacy: Privacy,
    pub(crate) dnd_until: Option<i64>,
    /// Apps whose notifications all show.
    pub(crate) expanded: std::collections::HashSet<String>,
    pub(crate) seeking: Option<f32>,
    /// Newest first.
    pub(crate) notifications: Vec<Notification>,
    /// The notifications shown in a popup since closed: the others are new.
    pub(crate) seen: std::collections::HashSet<u32>,
    pub(crate) brightness: Option<(i32, i32)>,
    pub(crate) power: Option<PowerProfiles>,
    pub(crate) do_not_disturb: bool,
    pub(crate) dark_mode: bool,
    pub(crate) show_percentage: bool,
    pub(crate) show_active: bool,
    /// Held while keeping the computer awake.
    pub(crate) inhibit: Option<u32>,
    pub(crate) touchpad: Option<services::Touchpad>,
    pub(crate) hot_edge: bool,
    pub(crate) bar_spacing: BarSpacing,
    pub(crate) hidden_icons: Hidden,
    /// The DNS servers last set by hand, to go back to.
    pub(crate) custom_dns: Vec<String>,
    pub(crate) icon_size: IconSize,
    /// The height of the screen the panel is on, in logical pixels.
    screen_height: Option<i32>,
    /// The panel button's size, all its icons.
    button_size: Option<iced::Size>,
    /// When a volume or brightness slider last moved. What PipeWire or the
    /// settings daemon tell meanwhile is the slider's own change, from a
    /// moment before: it would pull the slider back, and reload the list
    /// under it.
    adjusting: Option<std::time::Instant>,
}

impl App {
    pub(crate) fn high_contrast(&self) -> bool {
        self.core.system_theme().cosmic().is_high_contrast
    }

    pub(crate) fn airplane_mode(&self) -> bool {
        !self.network.wifi_enabled && (self.bluetooth.adapter.is_none() || !self.bluetooth.powered)
    }

    pub(crate) fn network_icon(&self) -> &'static str {
        match &self.network.connection {
            Some((_, kind)) if kind == "802-3-ethernet" => "network-wired-symbolic",
            Some((_, kind)) if kind == "802-11-wireless" => "network-wireless-signal-good-symbolic",
            Some(_) => "network-vpn-symbolic",
            None if self.airplane_mode() => "airplane-mode-symbolic",
            None if !self.network.wifi_enabled => "network-wireless-disabled-symbolic",
            None => "network-wireless-offline-symbolic",
        }
    }

    pub(crate) fn volume_icon(&self) -> &'static str {
        match &self.volume {
            None => "audio-volume-muted-symbolic",
            Some(v) if v.muted || v.level <= 0.0 => "audio-volume-muted-symbolic",
            Some(v) if v.level < 0.34 => "audio-volume-low-symbolic",
            Some(v) if v.level < 0.67 => "audio-volume-medium-symbolic",
            Some(_) => "audio-volume-high-symbolic",
        }
    }

    /// Whether a notification came since the popup was last open.
    pub(crate) fn is_new(&self, notification: &Notification) -> bool {
        !notification.restored && !self.seen.contains(&notification.id)
    }

    /// The player shown: the one picked, else the first, playing.
    pub(crate) fn shown_media(&self) -> Option<&Media> {
        self.player
            .as_ref()
            .and_then(|service| self.media.iter().find(|m| &m.service == service))
            .or(self.media.first())
    }

    fn remove_notification(&mut self, key: NotificationKey) {
        self.notifications.retain(|n| (n.id, n.restored) != key);
        notifications::save(&self.notifications);
    }

    pub(crate) fn new_count(&self) -> usize {
        self.notifications.iter().filter(|n| self.is_new(n)).count()
    }

    fn stream_mut(&mut self, app: &str) -> Option<&mut audio::Stream> {
        self.mixer
            .as_mut()?
            .streams
            .iter_mut()
            .find(|stream| stream.app == app)
    }

    fn adjusting(&self) -> bool {
        self.adjusting.is_some_and(|at| at.elapsed() < Duration::from_secs(2))
    }

    /// How tall the popup can be: as the screen allows.
    pub fn max_height(&self) -> f32 {
        self.screen_height
            .map_or(MAX_HEIGHT, |height| (height as f32 - SCREEN_MARGIN).clamp(480.0, 1200.0))
    }

    fn close_popup(&mut self) -> app::Task<Message> {
        self.mark_seen();
        self.page = None;
        match self.popup.take() {
            Some(id) => destroy_popup(id),
            None => Task::none(),
        }
    }

    fn mark_seen(&mut self) {
        self.seen = self
            .notifications
            .iter()
            .filter(|n| !n.restored)
            .map(|n| n.id)
            .collect();
        self.media_outputs = false;
    }

    fn save_tiles(&self) {
        let ids: Vec<String> = self.tiles.iter().map(|t| t.id()).collect();
        services::set_tiles(&ids);
    }

    /// What a page needs once shown.
    fn load(&self, page: Page) -> app::Task<Message> {
        match page {
            Page::Wifi => Task::batch([
                Task::future(async {
                    match services::network_now().await {
                        Some(network) => cosmic::action::app(Message::Update(Update::Network(network))),
                        None => cosmic::action::app(Message::None),
                    }
                }),
                load_wifi(false, Duration::ZERO),
                // A scan takes a few seconds.
                load_wifi(true, Duration::from_secs(3)),
            ]),
            Page::Sound => Task::batch([
                Task::future(async {
                    cosmic::action::app(Message::Sinks(services::sinks().await))
                }),
                load_mixer(),
            ]),
            Page::Displays => Task::future(async {
                cosmic::action::app(Message::Displays(services::displays().await))
            }),
            _ => Task::none(),
        }
    }
}

fn default_tiles() -> Vec<Placed> {
    Tile::DEFAULT
        .into_iter()
        .map(|tile| Placed { tile, small: false })
        .collect()
}

fn load_mixer() -> app::Task<Message> {
    Task::future(async { cosmic::action::app(Message::Mixer(audio::mixer().await)) })
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The steps of a scroll: a notch of the wheel is one.
pub fn scroll_steps(delta: iced::mouse::ScrollDelta) -> f32 {
    match delta {
        iced::mouse::ScrollDelta::Lines { y, .. } => y,
        iced::mouse::ScrollDelta::Pixels { y, .. } => y / 40.0,
    }
}

/// "2 h 40 min".
pub fn duration(seconds: i64) -> String {
    let minutes = (seconds + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn load_wifi(scan: bool, delay: Duration) -> app::Task<Message> {
    Task::future(async move {
        if scan {
            _ = services::wifi_networks(true).await;
        }
        tokio::time::sleep(delay).await;
        let networks = services::wifi_networks(false).await.ok().flatten();
        cosmic::action::app(Message::WifiNetworks(networks))
    })
}

fn spawn(program: &str, args: &[&str]) {
    crate::host::spawn(program, args);
}

fn run<F: Future<Output = zbus::Result<()>> + Send + 'static>(
    what: &'static str,
    future: F,
) -> app::Task<Message> {
    Task::future(async move {
        if let Err(err) = future.await {
            tracing::warn!("cannot {what}: {err}");
        }
        cosmic::action::app(Message::None)
    })
}

/// The popup under the whole button, flush with its end, as the Control
/// Center on macOS: by the screen's corner when the button is there. COSMIC
/// puts it under the button's first icon, which on this wide button left it
/// far from the corner.
fn line_up(
    positioner: &mut iced::platform_specific::runtime::wayland::popup::SctkPositioner,
    panel: cosmic::applet::cosmic_panel_config::PanelAnchor,
    button: iced::Size,
) {
    use cosmic::{
        applet::cosmic_panel_config::PanelAnchor,
        cctk::sctk::reexports::protocols::xdg::shell::client::xdg_positioner::{Anchor, Gravity},
    };
    positioner.anchor_rect.width = button.width.round() as i32;
    positioner.anchor_rect.height = button.height.round() as i32;
    // Its end along the panel; down the panel when it's at a side.
    (positioner.anchor, positioner.gravity) = match panel {
        PanelAnchor::Top => (Anchor::BottomRight, Gravity::BottomLeft),
        PanelAnchor::Bottom => (Anchor::TopRight, Gravity::TopLeft),
        PanelAnchor::Left => (Anchor::TopRight, Gravity::BottomRight),
        PanelAnchor::Right => (Anchor::TopLeft, Gravity::BottomLeft),
    };
}

/// A VPN on or off. One that won't come on, often for a password only
/// COSMIC Settings asks for, opens its page there.
fn connect_vpn(vpn: services::Vpn) -> app::Task<Message> {
    let turning_on = vpn.active.is_none();
    Task::future(async move {
        match services::connect_vpn(vpn).await {
            Err(err) if turning_on => {
                tracing::warn!("cannot turn the VPN on: {err}");
                cosmic::action::app(Message::Settings(Some("vpn")))
            }
            Err(err) => {
                tracing::warn!("cannot turn the VPN off: {err}");
                cosmic::action::app(Message::None)
            }
            Ok(()) => cosmic::action::app(Message::None),
        }
    })
}

fn popup_limits(max_height: f32) -> Limits {
    Limits::NONE
        .min_width(WIDTH)
        .max_width(WIDTH)
        .min_height(1.0)
        .max_height(max_height)
}

impl cosmic::Application for App {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = crate::APP_ID;

    fn init(core: app::Core, _flags: ()) -> (Self, app::Task<Message>) {
        let tiles = services::tiles()
            .map(|ids| ids.iter().filter_map(|id| Placed::from_id(id)).collect())
            .unwrap_or_else(default_tiles);
        let app = Self {
            core,
            tiles,
            do_not_disturb: DO_NOT_DISTURB.get(),
            dark_mode: DARK_MODE.get(),
            tiling_config: services::tiling_config(),
            show_percentage: SHOW_PERCENTAGE.get(),
            show_active: !HIDE_ACTIVE.get(),
            notifications: notifications::load(),
            dnd_until: services::dnd_until(),
            touchpad: services::touchpad(),
            hot_edge: services::hot_edge(),
            bar_spacing: BarSpacing::load(),
            hidden_icons: Hidden::load(),
            custom_dns: crate::dns::saved_custom(),
            icon_size: IconSize::load(),
            ..Self::default()
        };
        // COSMIC_QS_PREVIEW=sound opens the popup on that page by itself, to
        // take screenshots without clicking the panel.
        let preview = std::env::var("COSMIC_QS_PREVIEW")
            .ok()
            .and_then(|name| Page::from_name(&name));
        let task = match preview {
            Some(page) => Task::future(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                cosmic::action::app(Message::Preview(page))
            }),
            None => Task::none(),
        };
        (app, task)
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> app::Task<Message> {
        match message {
            Message::TogglePopup => {
                if self.popup.is_some() {
                    return self.close_popup();
                }
                let Some(parent) = self.core.main_window_id() else {
                    return Task::none();
                };
                let id = window::Id::unique();
                self.popup = Some(id);
                self.page = Some(Page::Main);
                return cosmic::surface::surface_task(cosmic::surface::action::app_popup(
                    |_: &Self| Default::default(),
                    move |app: &mut Self| {
                        let mut settings =
                            app.core
                                .applet
                                .get_popup_settings(parent, id, None, None, None);
                        settings.positioner.size_limits = popup_limits(app.max_height());
                        if let Some(size) = app.button_size {
                            line_up(&mut settings.positioner, app.core.applet.anchor, size);
                        }
                        settings
                    },
                    None,
                ));
            }
            Message::Preview(page) => {
                let open = self.update(Message::TogglePopup);
                self.page = Some(page);
                return Task::batch([open, self.load(page)]);
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.mark_seen();
                    self.popup = None;
                    self.page = None;
                }
            }
            Message::ShowPage(page) => {
                self.page = Some(page);
                self.selected_tile = None;
                return self.load(page);
            }
            Message::Update(update) => match update {
                Update::Network(network) => {
                    // Servers set by hand, kept before going to another choice.
                    if let Some(crate::dns::Choice::Custom(servers)) =
                        network.dns.as_ref().map(|d| &d.choice)
                    {
                        if *servers != self.custom_dns {
                            self.custom_dns = servers.clone();
                            crate::dns::save_custom(servers);
                        }
                    }
                    let changed = network != self.network;
                    self.network = network;
                    if changed && self.page == Some(Page::Wifi) {
                        return load_wifi(false, Duration::ZERO);
                    }
                }
                Update::Bluetooth(bluetooth) => self.bluetooth = bluetooth,
                Update::Battery(battery) => self.battery = battery,
                Update::Volume(_) | Update::Audio if self.adjusting() => {}
                Update::Volume(volume) => {
                    let changed = volume.is_some() != self.volume.is_some();
                    self.volume = volume;
                    if changed && self.page == Some(Page::Sound) {
                        return self.load(Page::Sound);
                    }
                }
                Update::Microphone(muted) => self.microphone = muted,
                Update::Audio => {
                    if self.page == Some(Page::Sound) {
                        return load_mixer();
                    }
                }
                Update::Peripherals(peripherals) => self.peripherals = peripherals,
                Update::Brightness(_) if self.adjusting() => {}
                Update::Brightness(brightness) => self.brightness = brightness,
                Update::PowerProfiles(power) => self.power = power,
            },
            Message::SetVolume(percent) => {
                self.adjusting = Some(std::time::Instant::now());
                let level = percent / 100.0;
                if let Some(volume) = self.volume.as_mut() {
                    volume.level = level;
                }
                return Task::future(async move {
                    services::set_volume(level).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::ToggleMute => {
                return Task::future(async {
                    services::toggle_mute().await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::ToggleMicrophone => {
                return Task::future(async {
                    services::toggle_microphone().await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::SetBrightness(percent) => {
                if let Some((current, max)) = self.brightness.as_mut() {
                    self.adjusting = Some(std::time::Instant::now());
                    // Never all the way to black.
                    *current = ((percent / 100.0 * *max as f32) as i32).max(*max / 100).max(1);
                    let value = *current;
                    return Task::future(async move {
                        services::set_brightness(value).await;
                        cosmic::action::app(Message::None)
                    });
                }
            }
            Message::ToggleWifi => {
                if self.network.wifi_enabled {
                    self.wifi_networks = None;
                }
                let enable = !self.network.wifi_enabled;
                let mut tasks = vec![run("toggle Wi-Fi", services::set_wifi(enable))];
                if enable && self.page == Some(Page::Wifi) {
                    tasks.push(load_wifi(true, Duration::from_secs(3)));
                }
                return Task::batch(tasks);
            }
            Message::WifiNetworks(networks) => self.wifi_networks = networks,
            Message::JoinWifi(index) => {
                let Some(networks) = &self.wifi_networks else {
                    return Task::none();
                };
                let Some(network) = networks.networks.get(index).cloned() else {
                    return Task::none();
                };
                let device = networks.device.clone();
                return Task::future(async move {
                    match services::join_wifi(device, network).await {
                        Ok(Joined::NeedsSettings) => {
                            cosmic::action::app(Message::Settings(Some("wireless")))
                        }
                        Ok(Joined::Connecting) => cosmic::action::app(Message::None),
                        Err(err) => {
                            tracing::warn!("cannot join network: {err}");
                            cosmic::action::app(Message::None)
                        }
                    }
                });
            }
            Message::ToggleBluetooth => {
                if let Some(adapter) = self.bluetooth.adapter.clone() {
                    return run(
                        "toggle Bluetooth",
                        services::set_bluetooth(adapter, !self.bluetooth.powered),
                    );
                }
            }
            Message::ConnectBluetooth(device, connect) => {
                return run(
                    "connect Bluetooth device",
                    services::connect_bluetooth(device, connect),
                );
            }
            Message::ToggleVpn => {
                let on: Vec<_> = self
                    .network
                    .vpns
                    .iter()
                    .filter(|vpn| vpn.active.is_some())
                    .cloned()
                    .collect();
                if on.is_empty() {
                    return match self.network.vpns.first() {
                        Some(vpn) => connect_vpn(vpn.clone()),
                        // None set up yet: where they are.
                        None => self.update(Message::Settings(Some("vpn"))),
                    };
                }
                return Task::batch(on.into_iter().map(connect_vpn));
            }
            Message::ConnectVpn(index) => {
                if let Some(vpn) = self.network.vpns.get(index) {
                    return connect_vpn(vpn.clone());
                }
            }
            Message::NextDns => {
                if let Some(dns) = &self.network.dns {
                    let next = crate::dns::next(&dns.choice, &self.custom_dns);
                    return self.update(Message::SetDns(next));
                }
            }
            Message::SetDns(choice) => {
                if let Some(dns) = self.network.dns.as_mut() {
                    let target = dns.clone();
                    // Shown at once; NetworkManager confirms it a moment later.
                    dns.choice = choice.clone();
                    return Task::future(async move {
                        if let Err(err) = crate::dns::set(target, choice).await {
                            tracing::warn!("cannot change the DNS servers: {err}");
                        }
                        match services::network_now().await {
                            Some(network) => cosmic::action::app(Message::Update(Update::Network(network))),
                            None => cosmic::action::app(Message::None),
                        }
                    });
                }
            }
            Message::NextPowerProfile => {
                if let Some(power) = &self.power {
                    let order = ["power-saver", "balanced", "performance"];
                    let available: Vec<&str> = order
                        .into_iter()
                        .filter(|p| power.available.iter().any(|a| a == p))
                        .collect();
                    let current = available.iter().position(|p| *p == power.active);
                    let next = current.map_or(0, |i| (i + 1) % available.len());
                    if let Some(next) = available.get(next) {
                        return run(
                            "change power profile",
                            services::set_power_profile((*next).to_owned()),
                        );
                    }
                }
            }
            Message::SetPowerProfile(profile) => {
                return run("change power profile", services::set_power_profile(profile));
            }
            Message::ToggleDoNotDisturb => {
                let dnd = if self.do_not_disturb { Dnd::Off } else { Dnd::Forever };
                return self.update(Message::SetDoNotDisturb(dnd));
            }
            Message::SetDoNotDisturb(dnd) => {
                use chrono::{Days, Local, NaiveTime};
                self.dnd_until = match dnd {
                    Dnd::Off | Dnd::Forever => None,
                    Dnd::For(seconds) => Some(now() + seconds),
                    Dnd::Tomorrow => Local::now()
                        .date_naive()
                        .checked_add_days(Days::new(1))
                        .and_then(|day| {
                            day.and_time(NaiveTime::from_hms_opt(8, 0, 0)?)
                                .and_local_timezone(Local)
                                .earliest()
                        })
                        .map(|t| t.timestamp()),
                };
                self.do_not_disturb = !matches!(dnd, Dnd::Off);
                DO_NOT_DISTURB.set(self.do_not_disturb);
                services::set_dnd_until(self.dnd_until);
            }
            Message::CheckDoNotDisturb => {
                if self.dnd_until.is_some_and(|until| now() >= until) {
                    return self.update(Message::SetDoNotDisturb(Dnd::Off));
                }
            }
            Message::ToggleDarkMode => {
                self.dark_mode = !self.dark_mode;
                DARK_MODE.set(self.dark_mode);
            }
            Message::ToggleAirplaneMode => {
                let radios_on = self.airplane_mode();
                let mut tasks = vec![run("toggle Wi-Fi", services::set_wifi(radios_on))];
                if let Some(adapter) = self.bluetooth.adapter.clone() {
                    tasks.push(run(
                        "toggle Bluetooth",
                        services::set_bluetooth(adapter, radios_on),
                    ));
                }
                return Task::batch(tasks);
            }
            Message::Output(name, height) => {
                tracing::debug!("output {name:?}: {height:?}");
                let panel = std::env::var("COSMIC_PANEL_OUTPUT").unwrap_or_default();
                // the panel's screen, or any if it's on all of them
                if name.as_deref() == Some(panel.as_str())
                    || (panel.is_empty() || panel == "all") && self.screen_height.is_none()
                {
                    self.screen_height = height;
                }
            }
            Message::Resized(id, size) => {
                if Some(id) == self.core.main_window_id() {
                    self.button_size = Some(size);
                }
            }
            Message::ToggleTouchpad => {
                let enabled = self.touchpad != Some(services::Touchpad::Enabled);
                self.touchpad = Some(if enabled {
                    services::Touchpad::Enabled
                } else {
                    services::Touchpad::Disabled
                });
                services::set_touchpad(enabled);
            }
            Message::ToggleHotEdge => {
                self.hot_edge = !self.hot_edge;
                services::set_hot_edge(self.hot_edge);
            }
            Message::OpenHotEdge => {
                spawn("cosmic-ext-hot-edge", &[]);
                return self.close_popup();
            }
            Message::ToggleKeepAwake => {
                return match self.inhibit.take() {
                    Some(cookie) => run("allow idling", services::allow_idle(cookie)),
                    None => Task::future(async {
                        let cookie = services::inhibit_idle(strings().keep_awake_reason)
                            .await
                            .inspect_err(|err| tracing::warn!("cannot keep awake: {err}"))
                            .ok();
                        cosmic::action::app(Message::Inhibited(cookie))
                    }),
                };
            }
            Message::Inhibited(cookie) => self.inhibit = cookie,
            Message::SettingsChanged => {
                self.do_not_disturb = DO_NOT_DISTURB.get();
                // Turned off elsewhere: no timer left.
                if !self.do_not_disturb && self.dnd_until.is_some() {
                    self.dnd_until = None;
                    services::set_dnd_until(None);
                }
                self.dark_mode = DARK_MODE.get();
                self.tiling_config = services::tiling_config();
                self.touchpad = services::touchpad();
                self.hot_edge = services::hot_edge();
            }
            Message::ToggleShowActive => {
                self.show_active = !self.show_active;
                HIDE_ACTIVE.set(!self.show_active);
            }
            Message::ToggleShowPercentage => {
                self.show_percentage = !self.show_percentage;
                SHOW_PERCENTAGE.set(self.show_percentage);
            }
            Message::Sinks(sinks) => self.sinks = sinks,
            Message::SetSink(name) => {
                if let Some((_, default)) = self.sinks.as_mut() {
                    *default = name.clone();
                }
                return Task::future(async move {
                    services::set_sink(name).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::Displays(displays) => self.displays = Some(displays),
            Message::Workspaces(event) => match event {
                workspaces::Event::Ready(tx) => self.workspaces_tx = Some(tx),
                workspaces::Event::Tiling(tiling) => self.workspace_tiling = Some(tiling),
            },
            Message::ToggleTiling => {
                if let (Some(tx), Some(tiling)) = (&self.workspaces_tx, self.workspace_tiling) {
                    // The workspace's tiling then follows its own setting.
                    services::set_tiling_config("autotile", self.tiling_config.new_workspaces_tile);
                    _ = tx.send(!tiling);
                    self.workspace_tiling = Some(!tiling);
                }
            }
            Message::SetNewWorkspacesTile(tile) => {
                self.tiling_config.new_workspaces_tile = tile;
                services::set_tiling_config("autotile", tile);
            }
            Message::ToggleActiveHint => {
                self.tiling_config.active_hint = !self.tiling_config.active_hint;
                services::set_tiling_config("active_hint", self.tiling_config.active_hint);
            }
            Message::A11y(event) => match event {
                services::A11y::Ready(tx) => self.a11y_tx = Some(tx),
                services::A11y::Event(event) => match event {
                    services::AccessibilityEvent::Magnifier(on) => self.magnifier = Some(on),
                    services::AccessibilityEvent::ScreenFilter { inverted, filter } => {
                        self.screen_filter = Some((inverted, filter));
                    }
                    services::AccessibilityEvent::Closed => {
                        self.a11y_tx = None;
                        self.magnifier = None;
                        self.screen_filter = None;
                    }
                    // Only changes are sent: until one, both are off.
                    services::AccessibilityEvent::Bound(_) => {
                        self.magnifier.get_or_insert(false);
                        self.screen_filter.get_or_insert((false, None));
                    }
                },
            },
            Message::ScreenReader(response) => {
                use cosmic_settings_accessibility_subscription::Response;
                match response {
                    Response::Init(enabled, tx) => {
                        self.screen_reader = Some(enabled);
                        self.screen_reader_tx = Some(tx);
                    }
                    Response::ScreenReader(enabled) => self.screen_reader = Some(enabled),
                    Response::IsEnabled(_) => {}
                    Response::Error(err) => tracing::debug!("screen reader: {err}"),
                }
            }
            Message::ToggleMagnifier => {
                if let (Some(tx), Some(on)) = (&self.a11y_tx, self.magnifier) {
                    _ = tx.send(services::AccessibilityRequest::Magnifier(!on));
                }
            }
            Message::ToggleInvertColors => {
                if let (Some(tx), Some((inverted, filter))) = (&self.a11y_tx, self.screen_filter) {
                    _ = tx.send(services::AccessibilityRequest::ScreenFilter {
                        inverted: !inverted,
                        filter,
                    });
                }
            }
            Message::ToggleScreenReader => {
                if let (Some(tx), Some(on)) = (&self.screen_reader_tx, self.screen_reader) {
                    _ = tx.send(cosmic_settings_accessibility_subscription::Request::ScreenReader(!on));
                    self.screen_reader = Some(!on);
                }
            }
            Message::ToggleHighContrast => {
                let enabled = !self.core.system_theme().cosmic().is_high_contrast;
                services::set_high_contrast(enabled);
            }
            Message::OpenWorkspaces => {
                spawn("cosmic-workspaces", &[]);
                return self.close_popup();
            }
            Message::Media(mut media) => {
                // Browsers leave the length out of what they tell for a moment
                // while seeking, and may say nothing more after: the progress
                // bar went with it until playback paused or resumed.
                for m in media.iter_mut().filter(|m| m.length <= 0) {
                    if let Some(old) = self
                        .media
                        .iter()
                        .find(|old| old.service == m.service && old.title == m.title)
                    {
                        m.length = old.length;
                    }
                }
                // A player starting to play comes forward, as on a phone.
                let started = media.iter().find(|m| {
                    m.playing
                        && !self
                            .media
                            .iter()
                            .any(|old| old.service == m.service && old.playing)
                });
                if let Some(started) = started {
                    self.player = Some(started.service.clone());
                }
                self.media = media;
                self.seeking = None;
            }
            Message::SelectPlayer(service) => {
                self.player = Some(service);
                self.seeking = None;
            }
            Message::ToggleMediaOutputs => {
                self.media_outputs = !self.media_outputs;
                if self.media_outputs {
                    return Task::future(async {
                        cosmic::action::app(Message::Sinks(services::sinks().await))
                    });
                }
            }
            Message::MediaControl(control) => {
                if let Some(media) = self.shown_media() {
                    return run("control the player", media::control(media.service.clone(), control));
                }
            }
            Message::Seek(position) => self.seeking = Some(position),
            Message::SeekDone => {
                let service = self.shown_media().map(|m| m.service.clone());
                let media = self
                    .media
                    .iter_mut()
                    .find(|m| Some(&m.service) == service.as_ref());
                let (Some(position), Some(media)) = (self.seeking.take(), media) else {
                    return Task::none();
                };
                media.position = position as i64;
                media.at = std::time::Instant::now();
                if let Some(track) = media.track.clone() {
                    return run(
                        "seek",
                        media::seek(media.service.clone(), track, position as i64),
                    );
                }
            }
            Message::Tick => {}
            Message::Notification(event) => match event {
                notifications::Event::New(notification) => {
                    // A notification replacing another keeps its id, and is new.
                    self.notifications.retain(|n| n.restored || n.id != notification.id);
                    self.seen.remove(&notification.id);
                    self.notifications.insert(0, notification);
                    notifications::save(&self.notifications);
                }
                notifications::Event::Closed(id) => self.remove_notification((id, false)),
                notifications::Event::Dismissed(id) => {
                    if let Some(notification) =
                        self.notifications.iter_mut().find(|n| !n.restored && n.id == id)
                    {
                        notification.restored = true;
                        notification.actions.clear();
                        notifications::save(&self.notifications);
                    }
                }
            },
            Message::OpenNotification(key) => {
                self.remove_notification(key);
                let (id, restored) = key;
                if restored {
                    return Task::none();
                }
                let close = self.close_popup();
                return Task::batch([
                    close,
                    Task::future(async move {
                        notifications::invoke(id, "default".to_owned()).await;
                        notifications::dismiss(vec![id]).await;
                        cosmic::action::app(Message::None)
                    }),
                ]);
            }
            Message::DismissNotification(key) => {
                self.remove_notification(key);
                let (id, restored) = key;
                if restored {
                    return Task::none();
                }
                return Task::future(async move {
                    notifications::dismiss(vec![id]).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::NotificationAction(id, action) => {
                self.remove_notification((id, false));
                return Task::future(async move {
                    notifications::invoke(id, action).await;
                    notifications::dismiss(vec![id]).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::ToggleGroup(app) => {
                if !self.expanded.remove(&app) {
                    self.expanded.insert(app);
                }
            }
            Message::ClearNotifications => {
                let ids: Vec<u32> = self
                    .notifications
                    .drain(..)
                    .filter(|n| !n.restored)
                    .map(|n| n.id)
                    .collect();
                notifications::save(&self.notifications);
                return Task::future(async move {
                    notifications::dismiss(ids).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::SetScale(index, scale) => {
                let Some(display) = self.displays.as_mut().and_then(|d| d.get_mut(index)) else {
                    return Task::none();
                };
                display.scale = scale;
                let display = display.clone();
                return Task::future(async move {
                    services::set_scale(display, scale).await;
                    cosmic::action::app(Message::Displays(services::displays().await))
                });
            }
            Message::SetDdcBrightness(index, percent) => {
                if let Some((_, current, max)) = self
                    .displays
                    .as_mut()
                    .and_then(|d| d.get_mut(index))
                    .and_then(|d| d.ddc.as_mut())
                {
                    *current = (percent / 100.0 * *max as f32).round() as u32;
                }
            }
            Message::ApplyDdcBrightness(index) => {
                // DDC is slow: only once let go.
                if let Some((number, current, _)) = self
                    .displays
                    .as_ref()
                    .and_then(|d| d.get(index))
                    .and_then(|d| d.ddc)
                {
                    return Task::future(async move {
                        services::set_ddc_brightness(number, current).await;
                        cosmic::action::app(Message::None)
                    });
                }
            }
            Message::DragStart(index) => {
                self.drag = Some(Drag {
                    index,
                    moved: false,
                });
            }
            Message::DragOver(index) => {
                // The others make way as it passes over them.
                if let Some(drag) = self.drag.as_mut() {
                    if drag.index != index && index < self.tiles.len() {
                        let placed = self.tiles.remove(drag.index);
                        self.tiles.insert(index, placed);
                        drag.index = index;
                        drag.moved = true;
                        self.selected_tile = None;
                    }
                }
            }
            Message::DragEnd => {
                if let Some(drag) = self.drag.take() {
                    if drag.moved {
                        self.save_tiles();
                    } else {
                        // A click: choose it to move or resize, or let go of it.
                        self.selected_tile = match self.selected_tile {
                            Some(selected) if selected == drag.index => None,
                            _ => Some(drag.index),
                        };
                    }
                }
            }
            Message::MoveTile(index, by) => {
                let to = index.saturating_add_signed(by);
                if to < self.tiles.len() && to != index {
                    self.tiles.swap(index, to);
                    self.selected_tile = Some(to);
                    self.save_tiles();
                }
            }
            Message::ResizeTile(index) => {
                if let Some(placed) = self.tiles.get_mut(index) {
                    placed.small = !placed.small;
                    self.save_tiles();
                }
            }
            Message::RemoveTile(index) => {
                if index < self.tiles.len() {
                    self.tiles.remove(index);
                    self.selected_tile = None;
                    self.save_tiles();
                }
            }
            Message::AddTile(tile) => {
                if !self.tiles.iter().any(|p| p.tile == tile) {
                    self.tiles.push(Placed { tile, small: false });
                    self.save_tiles();
                }
            }
            Message::ResetTiles => {
                self.tiles = default_tiles();
                self.selected_tile = None;
                self.save_tiles();
            }
            Message::SetBarSpacing(spacing) => {
                self.bar_spacing = spacing;
                spacing.save();
            }
            Message::TogglePanelIcon(icon) => {
                self.hidden_icons.toggle(&icon);
                self.hidden_icons.save();
            }
            Message::SetIconSize(size) => {
                self.icon_size = size;
                size.save();
            }
            Message::Settings(page) => {
                spawn("cosmic-settings", page.as_slice());
                return self.close_popup();
            }
            Message::Screenshot => {
                spawn("cosmic-screenshot", &[]);
                return self.close_popup();
            }
            Message::Lock => {
                spawn("loginctl", &["lock-session"]);
                return self.close_popup();
            }
            Message::PowerOff => {
                spawn("cosmic-osd", &["shutdown"]);
                return self.close_popup();
            }
            Message::Restart => {
                spawn("cosmic-osd", &["restart"]);
                return self.close_popup();
            }
            Message::LogOut => {
                spawn("cosmic-osd", &["log-out"]);
                return self.close_popup();
            }
            Message::Suspend => {
                spawn("systemctl", &["suspend"]);
                return self.close_popup();
            }
            Message::Mixer(_) if self.adjusting() => {}
            Message::Mixer(mixer) => self.mixer = Some(mixer),
            Message::AdjustDone => {
                self.adjusting = None;
                if self.page == Some(Page::Sound) {
                    return load_mixer();
                }
            }
            Message::SetStreamVolume(app, percent) => {
                self.adjusting = Some(std::time::Instant::now());
                let Some(stream) = self.stream_mut(&app) else {
                    return Task::none();
                };
                stream.level = percent / 100.0;
                let (indices, level) = (stream.indices.clone(), stream.level);
                return Task::future(async move {
                    audio::set_stream_volume(indices, level).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::ToggleStreamMute(app) => {
                let Some(stream) = self.stream_mut(&app) else {
                    return Task::none();
                };
                stream.muted = !stream.muted;
                let (indices, muted) = (stream.indices.clone(), stream.muted);
                return Task::future(async move {
                    audio::set_stream_mute(indices, muted).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::SetSource(name) => {
                if let Some(mixer) = self.mixer.as_mut() {
                    mixer.default_source = name.clone();
                }
                return Task::future(async move {
                    audio::set_source(name).await;
                    cosmic::action::app(Message::Mixer(audio::mixer().await))
                });
            }
            Message::SetInputVolume(percent) => {
                self.adjusting = Some(std::time::Instant::now());
                let level = percent / 100.0;
                if let Some(input) = self.mixer.as_mut().and_then(|m| m.input.as_mut()) {
                    input.level = level;
                }
                return Task::future(async move {
                    audio::set_input_volume(level).await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::ToggleInputMute => {
                if let Some(input) = self.mixer.as_mut().and_then(|m| m.input.as_mut()) {
                    input.muted = !input.muted;
                }
                return Task::future(async {
                    services::toggle_microphone().await;
                    cosmic::action::app(Message::None)
                });
            }
            Message::Privacy(privacy) => self.privacy = privacy,
            Message::ScrollVolume(steps) => {
                if let Some(volume) = &self.volume {
                    let percent = (volume.level * 100.0 + steps * 5.0).clamp(0.0, 100.0);
                    return self.update(Message::SetVolume(percent));
                }
            }
            Message::ScrollBrightness(steps) => {
                if let Some((current, max)) = self.brightness {
                    let percent = (current as f32 * 100.0 / max as f32 + steps * 5.0).clamp(0.0, 100.0);
                    return self.update(Message::SetBrightness(percent));
                }
            }
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(action)));
            }
            Message::None => {}
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            services::subscription().map(Message::Update),
            services::settings_subscription().map(|()| Message::SettingsChanged),
            workspaces::subscription().map(Message::Workspaces),
            services::a11y_subscription().map(Message::A11y),
            cosmic_settings_accessibility_subscription::subscription().map(Message::ScreenReader),
            media::subscription().map(Message::Media),
            audio::privacy_subscription().map(Message::Privacy),
            notifications::subscription().map(Message::Notification),
        ];
        // The screen's size, for the popup's height.
        subscriptions.push(event::listen_with(|event, _, _| {
            use iced::event::wayland::{Event as Wayland, OutputEvent};
            match event {
                iced::Event::PlatformSpecific(iced::event::PlatformSpecific::Wayland(
                    Wayland::Output(
                        OutputEvent::Created(Some(info)) | OutputEvent::InfoUpdate(info),
                        _,
                    ),
                )) => Some(Message::Output(info.name.clone(), info.logical_size.map(|(_, h)| h))),
                _ => None,
            }
        }));
        subscriptions.push(window::resize_events().map(|(id, size)| Message::Resized(id, size)));
        if self.drag.is_some() {
            // Let go anywhere, on a tile or not.
            subscriptions.push(event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::DragEnd),
                _ => None,
            }));
        }
        if self.dnd_until.is_some() {
            subscriptions.push(
                iced::time::every(Duration::from_secs(20)).map(|_| Message::CheckDoNotDisturb),
            );
        }
        if self.popup.is_some() && self.shown_media().is_some_and(|m| m.playing) {
            subscriptions
                .push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }
        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        let t = strings();
        let applet = &self.core.applet;
        let size = self.icon_size.apply(applet.suggested_size(true).0);
        let (major, minor) = applet.suggested_padding(true);
        let colored = |name: &str, color: Option<fn(&cosmic::Theme) -> iced::Color>| -> Element<'_, Message> {
            widget::icon::icon(icon(name))
                .size(size)
                .class(theme::Svg::Custom(std::rc::Rc::new(move |theme| {
                    iced::widget::svg::Style {
                        color: Some(color.map_or_else(
                            || theme.cosmic().background(theme.transparent).on.into(),
                            |color| color(theme),
                        )),
                    }
                })))
                .into()
        };
        let panel_icon = |name: &str| colored(name, None);
        // Each icon with what it means on hover.
        let tip = |content: Element<'static, Message>, text: String| -> Element<'static, Message> {
            applet
                .applet_tooltip(content, text, self.popup.is_some(), Message::Surface, None)
                .into()
        };
        // On a panel at a side of the screen, the icons go in a column, and
        // what an icon says goes under it, smaller, to fit the panel's width.
        let horizontal = applet.is_horizontal();
        let with_text = |icon: Element<'static, Message>, text: String, accent: bool| -> Element<'static, Message> {
            let label = if horizontal {
                applet.text(text)
            } else {
                widget::text::caption(text)
            };
            let label = if accent { label.class(theme::Text::Accent).into() } else { label.into() };
            if horizontal {
                widget::row::with_children(vec![icon, label])
                    .spacing(2)
                    .align_y(Alignment::Center)
                    .into()
            } else {
                widget::column::with_children(vec![icon, label])
                    .align_x(Alignment::Center)
                    .into()
            }
        };

        let mut icons: Vec<Element<'_, Message>> = Vec::with_capacity(10);
        // Who is listening or watching, in the colors phones use.
        let privacy = &self.privacy;
        if !privacy.camera.is_empty() {
            let green: fn(&cosmic::Theme) -> iced::Color =
                |theme| theme.cosmic().success_color().into();
            icons.push(tip(
                colored("camera-web-symbolic", Some(green)),
                format!("{} · {}", privacy.camera.join(", "), t.using_camera),
            ));
        }
        if !privacy.microphone.is_empty() {
            let orange: fn(&cosmic::Theme) -> iced::Color =
                |theme| theme.cosmic().warning_color().into();
            icons.push(tip(
                colored("audio-input-microphone-symbolic", Some(orange)),
                format!("{} · {}", privacy.microphone.join(", "), t.using_microphone),
            ));
        }
        // Notifications: a bell, lit with how many are new.
        if !self.notifications.is_empty() && self.hidden_icons.shows(&PanelIcon::Notifications) {
            let new = self.new_count();
            if new == 0 {
                icons.push(tip(
                    panel_icon("notification-symbolic"),
                    format!("{} {}", self.notifications.len(), t.notifications.to_lowercase()),
                ));
            } else {
                let accent: fn(&cosmic::Theme) -> iced::Color =
                    |theme| theme.cosmic().accent_color().into();
                icons.push(tip(
                    with_text(colored("notification-new-symbolic", Some(accent)), new.to_string(), true),
                    format!(
                        "{new} {}",
                        if new == 1 { t.new_notification } else { t.new_notifications }
                    ),
                ));
            }
        }
        if self.show_active {
            for (name, title) in self.active_tiles() {
                icons.push(tip(panel_icon(name), title));
            }
        }
        let network = match &self.network.connection {
            Some((name, _)) => name.clone(),
            None if self.airplane_mode() => t.airplane_mode.to_owned(),
            None => t.disconnected.to_owned(),
        };
        if self.hidden_icons.shows(&PanelIcon::Network) {
            icons.push(tip(panel_icon(self.network_icon()), network));
        }
        if let Some(volume) = self.volume.as_ref().filter(|_| self.hidden_icons.shows(&PanelIcon::Volume)) {
            let text = if volume.muted {
                format!("{}: {}", t.volume, t.muted)
            } else {
                format!("{}: {:.0}%", t.volume, volume.level * 100.0)
            };
            // Scroll for louder or quieter; the middle button mutes. The
            // area takes the left button too, so it opens the popup itself.
            let area = widget::mouse_area(panel_icon(self.volume_icon()))
                .on_press(Message::TogglePopup)
                .on_scroll(|delta| Message::ScrollVolume(scroll_steps(delta)))
                .on_middle_press(Message::ToggleMute);
            icons.push(tip(area.into(), text));
        }
        // Each mouse, keyboard, headset… with its charge.
        for peripheral in self
            .peripherals
            .iter()
            .filter(|p| self.hidden_icons.shows(&PanelIcon::Peripheral(p.name.clone())))
        {
            icons.push(tip(
                with_text(
                    panel_icon(&peripheral.icon),
                    format!("{:.0}%", peripheral.percentage),
                    false,
                ),
                format!("{}: {:.0}%", peripheral.name, peripheral.percentage),
            ));
        }
        // The computer's battery last, by the screen's edge; scrolling on it
        // sets the screen's brightness.
        if let Some(battery) = self.battery.as_ref().filter(|_| self.hidden_icons.shows(&PanelIcon::Battery)) {
            let icon = panel_icon(&symbolic(&battery.icon));
            let content = if self.show_percentage {
                with_text(icon, format!("{:.0}%", battery.percentage), false)
            } else {
                icon
            };
            let mut text = format!("{}: {:.0}%", t.battery, battery.percentage);
            match battery.time_left {
                Some(left) if battery.charging => {
                    text.push_str(&format!(" · {} {}", duration(left), t.until_full));
                }
                Some(left) => text.push_str(&format!(" · {} {}", duration(left), t.left)),
                None if battery.charging && battery.percentage >= 99.0 => {
                    text.push_str(&format!(" · {}", t.charged));
                }
                None => {}
            }
            let area = widget::mouse_area(content)
                .on_press(Message::TogglePopup)
                .on_scroll(|delta| Message::ScrollBrightness(scroll_steps(delta)));
            icons.push(tip(area.into(), text));
        }
        // Everything hidden: still something to click.
        if icons.is_empty() {
            icons.push(tip(panel_icon("emblem-system-symbolic"), t.settings.to_owned()));
        }
        let icons: Element<'_, Message> = if horizontal {
            widget::row::with_children(icons)
                .spacing(self.bar_spacing.gap())
                .align_y(Alignment::Center)
                .into()
        } else {
            widget::column::with_children(icons)
                .spacing(self.bar_spacing.gap())
                .align_x(Alignment::Center)
                .into()
        };
        let button = button::custom(icons)
            .padding(if horizontal { [minor, major] } else { [major, minor] })
            .class(theme::Button::AppletIcon)
            .on_press_down(Message::TogglePopup);
        applet.autosize_window(button).into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return widget::text("").into();
        }
        let page = self.page.unwrap_or(Page::Main);
        let content = match page {
            Page::Main => self.main_page(),
            Page::Edit => pages::panel(self.edit_page()),
            Page::Wifi => pages::panel(self.wifi_page()),
            Page::Vpn => pages::panel(self.vpn_page()),
            Page::Bluetooth => pages::panel(self.bluetooth_page()),
            Page::Power => pages::panel(self.power_page()),
            Page::Sound => pages::panel(self.sound_page()),
            Page::Displays => pages::panel(self.displays_page()),
            Page::Tiling => pages::panel(self.tiling_page()),
            Page::DoNotDisturb => pages::panel(self.dnd_page()),
            Page::Session => pages::panel(self.session_page()),
            Page::Dns => pages::panel(self.dns_page()),
        };
        let depth = if page == Page::Main { 0 } else { 1 };
        // The pages draw their own panels, apart, instead of the applet's
        // popup, all one: as applets' popups are sized.
        cosmic::widget::autosize::autosize(
            Transition::new(page as u64, depth, content),
            cosmic::widget::Id::new("cosmic-applet-autosize"),
        )
            .limits(popup_limits(self.max_height()))
            .into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::Closed(id))
    }
}
