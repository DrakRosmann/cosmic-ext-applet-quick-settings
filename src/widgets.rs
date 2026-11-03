// SPDX-License-Identifier: GPL-3.0-only

//! The quick settings' own widgets, as phones draw them: a tile's surface,
//! which eases between off and on and gives a little as it's pressed, and a
//! thick slider with its icon inside.

use std::time::{Duration, Instant};

use cosmic::iced::{
    Border, Color, Event, Length, Padding, Point, Rectangle, Size, Vector,
    advanced::{
        Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, overlay, renderer,
        widget::{Operation, Tree, tree},
    },
    window,
};

/// Off to on and back.
const LIGHT: Duration = Duration::from_millis(200);
/// Pressed in and let go.
const PRESS: Duration = Duration::from_millis(120);
const RELEASE: Duration = Duration::from_millis(260);
/// A tile's corners: rounded, as phones' tiles, unless the theme is squarer.
pub fn tile_radius(theme: &cosmic::Theme) -> f32 {
    theme.cosmic().radius_xl()[0].min(20.0)
}
/// How much rounder or squarer a pressed tile gets, of its radius.
const PRESSED_RADIUS: f32 = 0.55;

/// Eases from one value to another over a while.
#[derive(Clone, Copy)]
struct Ease {
    from: f32,
    to: f32,
    start: Instant,
    length: Duration,
}

impl Ease {
    fn at(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: Instant::now(),
            length: Duration::ZERO,
        }
    }

    fn value(&self) -> f32 {
        if self.length.is_zero() {
            return self.to;
        }
        let t = (self.start.elapsed().as_secs_f32() / self.length.as_secs_f32()).min(1.0);
        // ease out cubic: quick, then settling
        let t = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * t
    }

    fn moving(&self) -> bool {
        self.start.elapsed() < self.length
    }

    fn go(&mut self, to: f32, length: Duration) {
        if to != self.to {
            *self = Self {
                from: self.value(),
                to,
                start: Instant::now(),
                length,
            };
        }
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// How bright a color looks, as WCAG measures it.
fn luminance(color: Color) -> f32 {
    let linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// What reads on `background`: the theme's `on`, unless it hardly shows
/// there, as its white on a white or light accent; then black or white,
/// whichever shows more.
pub fn readable(on: impl Into<Color>, background: impl Into<Color>) -> Color {
    let (on, background) = (on.into(), background.into());
    if contrast(on, background) >= 3.0 {
        return on;
    }
    let dark = Color::from_rgb(0.08, 0.08, 0.08);
    let light = Color::from_rgb(0.97, 0.97, 0.97);
    if contrast(dark, background) >= contrast(light, background) {
        dark
    } else {
        light
    }
}

/// For a container inside a tile or a colored button: it keeps the colors
/// they give, where COSMIC's plain one sets the panel's own, white on a
/// light accent or on the media card's white play button.
pub fn inherit_colors() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|_| cosmic::widget::container::Style::default())
}

/// The colors of a tile, off and on: background, hovered background, and
/// what's on it.
fn tile_colors(theme: &cosmic::Theme, on: bool) -> (Color, Color, Color) {
    let cosmic = theme.cosmic();
    let component = if on {
        &cosmic.accent_button
    } else {
        &cosmic.background(theme.transparent).component
    };
    (
        component.base.into(),
        component.hover.into(),
        readable(component.on, component.base),
    )
}

/// A tile's surface: lit when on, easing between off and on; a click (on
/// what inside doesn't take it) toggles it, a right click opens more of it.
pub struct Surface<'a, Message> {
    content: cosmic::Element<'a, Message>,
    on: bool,
    on_press: Option<Message>,
    on_right_press: Option<Message>,
    width: Length,
    height: Length,
    padding: Padding,
    /// Rounder than the theme's, or a pill: half the height.
    pill: bool,
}

impl<'a, Message> Surface<'a, Message> {
    pub fn new(content: impl Into<cosmic::Element<'a, Message>>, on: bool) -> Self {
        Self {
            content: content.into(),
            on,
            on_press: None,
            on_right_press: None,
            width: Length::Fill,
            height: Length::Fill,
            padding: Padding::ZERO,
            pill: false,
        }
    }

    pub fn on_press(mut self, message: Option<Message>) -> Self {
        self.on_press = message;
        self
    }

    pub fn on_right_press(mut self, message: Option<Message>) -> Self {
        self.on_right_press = message;
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }

    pub fn pill(mut self) -> Self {
        self.pill = true;
        self
    }
}

struct SurfaceState {
    lit: Ease,
    pressed: Ease,
    is_pressed: bool,
}

impl<Message: Clone> Widget<Message, cosmic::Theme, cosmic::Renderer> for Surface<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SurfaceState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SurfaceState {
            lit: Ease::at(if self.on { 1.0 } else { 0.0 }),
            pressed: Ease::at(0.0),
            is_pressed: false,
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<SurfaceState>();
        state.lit.go(if self.on { 1.0 } else { 0.0 }, LIGHT);
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::padded(limits, self.width, self.height, self.padding, |limits| {
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits)
        })
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let state = tree.state.downcast_mut::<SurfaceState>();
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            if state.lit.moving() || state.pressed.moving() {
                shell.request_redraw();
            }
            return;
        }
        if shell.is_event_captured() {
            return;
        }
        let over = cursor.is_over(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                if self.on_press.is_some() {
                    state.is_pressed = true;
                    state.pressed.go(1.0, PRESS);
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.is_pressed => {
                state.is_pressed = false;
                state.pressed.go(0.0, RELEASE);
                shell.request_redraw();
                if over && let Some(message) = &self.on_press {
                    shell.publish(message.clone());
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if over => {
                if let Some(message) = &self.on_right_press {
                    shell.publish(message.clone());
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => shell.request_redraw(),
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if !bounds.intersects(viewport) {
            return;
        }
        let state = tree.state.downcast_ref::<SurfaceState>();
        let lit = state.lit.value();
        let hovered = cursor.is_over(bounds) && self.on_press.is_some();
        let (off_base, off_hover, off_on) = tile_colors(theme, false);
        let (on_base, on_hover, on_on) = tile_colors(theme, true);
        let background = if hovered {
            mix(off_hover, on_hover, lit)
        } else {
            mix(off_base, on_base, lit)
        };
        let foreground = mix(off_on, on_on, lit);
        let full = if self.pill {
            bounds.height / 2.0
        } else {
            tile_radius(theme).min(bounds.height / 2.0)
        };
        let radius = full * (1.0 - (1.0 - PRESSED_RADIUS) * state.pressed.value());
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            background,
        );
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            &renderer::Style {
                icon_color: foreground,
                text_color: foreground,
                scale_factor: style.scale_factor,
            },
            child(layout),
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        let inner = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child(layout),
            cursor,
            viewport,
            renderer,
        );
        if inner == mouse::Interaction::None
            && self.on_press.is_some()
            && cursor.is_over(layout.bounds())
        {
            mouse::Interaction::Pointer
        } else {
            inner
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], child(layout), renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &cosmic::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, cosmic::Theme, cosmic::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            child(layout),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: Clone + 'a> From<Surface<'a, Message>> for cosmic::Element<'a, Message> {
    fn from(surface: Surface<'a, Message>) -> Self {
        Self::new(surface)
    }
}

/// A thick slider, its icon inside at the start: the part up to the level is
/// lit, a pill that starts as a circle around the icon. Drag anywhere or
/// scroll on it; a click on the icon does what it says (mute, say).
pub struct Level<'a, Message> {
    icon: cosmic::Element<'a, Message>,
    /// From 0 to 1.
    value: f32,
    on_change: Box<dyn Fn(f32) -> Message + 'a>,
    on_release: Option<Message>,
    on_icon_press: Option<Message>,
    /// Shown greyed, like a muted volume.
    dimmed: bool,
    height: f32,
}

impl<'a, Message> Level<'a, Message> {
    pub fn new(
        icon: impl Into<cosmic::Element<'a, Message>>,
        value: f32,
        on_change: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self {
            icon: icon.into(),
            value: value.clamp(0.0, 1.0),
            on_change: Box::new(on_change),
            on_release: None,
            on_icon_press: None,
            dimmed: false,
            height: 44.0,
        }
    }

    pub fn on_release(mut self, message: Message) -> Self {
        self.on_release = Some(message);
        self
    }

    pub fn on_icon_press(mut self, message: Option<Message>) -> Self {
        self.on_icon_press = message;
        self
    }

    pub fn dimmed(mut self, dimmed: bool) -> Self {
        self.dimmed = dimmed;
        self
    }

    /// The square at the start, where the icon is.
    fn icon_bounds(&self, bounds: Rectangle) -> Rectangle {
        Rectangle::new(bounds.position(), Size::new(bounds.height, bounds.height))
    }

    /// The level at `x`: the lit pill's end follows the pointer, past the
    /// icon's circle.
    fn level_at(&self, bounds: Rectangle, x: f32) -> f32 {
        let start = bounds.x + bounds.height / 2.0;
        let span = (bounds.width - bounds.height).max(1.0);
        ((x - start) / span).clamp(0.0, 1.0)
    }
}

#[derive(Default)]
struct LevelState {
    dragging: bool,
    /// Pressed on the icon, not moved yet: a click on it.
    on_icon: Option<Point>,
}

/// Moving this far from a press on the icon makes it a drag.
const DRAG_START: f32 = 4.0;

impl<Message: Clone> Widget<Message, cosmic::Theme, cosmic::Renderer> for Level<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<LevelState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(LevelState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.icon)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.icon));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits
            .width(Length::Fill)
            .height(Length::Fixed(self.height))
            .resolve(Length::Fill, Length::Fixed(self.height), Size::ZERO);
        let square = Size::new(self.height, self.height);
        let icon = self
            .icon
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &layout::Limits::new(Size::ZERO, square));
        let offset = Point::new(
            (square.width - icon.size().width) / 2.0,
            (square.height - icon.size().height) / 2.0,
        );
        layout::Node::with_children(size, vec![icon.move_to(offset)])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &cosmic::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<LevelState>();
        let bounds = layout.bounds();
        let set = |shell: &mut Shell<'_, Message>, x: f32| {
            shell.publish((self.on_change)(self.level_at(bounds, x)));
        };
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };
                if self.on_icon_press.is_some() && self.icon_bounds(bounds).contains(position) {
                    state.on_icon = Some(position);
                } else {
                    state.dragging = true;
                    set(shell, position.x);
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(pressed) = state.on_icon
                    && position.distance(pressed) > DRAG_START
                {
                    state.on_icon = None;
                    state.dragging = true;
                }
                if state.dragging {
                    set(shell, position.x);
                    shell.capture_event();
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.on_icon.take().is_some() {
                    if let Some(message) = &self.on_icon_press {
                        shell.publish(message.clone());
                    }
                    shell.capture_event();
                } else if state.dragging {
                    state.dragging = false;
                    if let Some(message) = &self.on_release {
                        shell.publish(message.clone());
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let steps = crate::app::scroll_steps(*delta);
                if steps != 0.0 {
                    let value = (self.value + steps * 0.05).clamp(0.0, 1.0);
                    shell.publish((self.on_change)(value));
                    if let Some(message) = &self.on_release {
                        shell.publish(message.clone());
                    }
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<LevelState>();
        let cosmic = theme.cosmic();
        let component = &cosmic.background(theme.transparent).component;
        let hovered = cursor.is_over(bounds) || state.dragging;
        let track: Color = if hovered { component.hover } else { component.base }.into();
        let radius = bounds.height / 2.0;
        // Round ends, or as square as the theme's corners.
        let corner = cosmic.radius_xl()[0].min(radius);
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: corner.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            track,
        );
        let (lit, on): (Color, Color) = if self.dimmed {
            (component.pressed.into(), component.on.into())
        } else {
            let accent = &cosmic.accent_button;
            (accent.base.into(), readable(accent.on, accent.base))
        };
        let width = bounds.height + (bounds.width - bounds.height) * self.value;
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(bounds.position(), Size::new(width, bounds.height)),
                border: Border {
                    radius: corner.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            lit,
        );
        // A knob at the lit part's end, like phones draw it, once it's past
        // the icon.
        let knob = Size::new(4.0, bounds.height * 0.42);
        if width - bounds.height > 12.0 {
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle::new(
                    Point::new(
                        bounds.x + width - radius / 2.0 - knob.width / 2.0 - 2.0,
                        bounds.center_y() - knob.height / 2.0,
                    ),
                    knob,
                ),
                border: Border {
                    radius: (knob.width / 2.0).into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            Color { a: 0.55, ..on },
        );
        }
        let icon_layout = layout
            .children()
            .next()
            .expect("laid out with its icon")
            .with_virtual_offset(layout.virtual_offset());
        self.icon.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            &renderer::Style {
                icon_color: on,
                text_color: on,
                scale_factor: style.scale_factor,
            },
            icon_layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<LevelState>();
        let bounds = layout.bounds();
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if let Some(position) = cursor.position_over(bounds) {
            if self.on_icon_press.is_some() && self.icon_bounds(bounds).contains(position) {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::Grab
            }
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message: Clone + 'a> From<Level<'a, Message>> for cosmic::Element<'a, Message> {
    fn from(level: Level<'a, Message>) -> Self {
        Self::new(level)
    }
}

fn child(layout: Layout<'_>) -> Layout<'_> {
    layout
        .children()
        .next()
        .expect("laid out with its content")
        .with_virtual_offset(layout.virtual_offset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_on_any_accent() {
        let white = Color::from_rgb(1.0, 1.0, 1.0);
        // White on a white or light cyan accent: dark instead.
        assert!(luminance(readable(white, white)) < 0.1);
        assert!(luminance(readable(white, Color::from_rgb(0.58, 0.92, 0.92))) < 0.1);
        // The cyan of #5, under the panel's light grey.
        let grey = Color::from_rgb(0.78, 0.78, 0.78);
        assert!(luminance(readable(grey, Color::from_rgb(0.39, 0.82, 0.87))) < 0.1);
        // The theme's pick, where it shows.
        let blue = Color::from_rgb(0.1, 0.3, 0.7);
        assert_eq!(readable(white, blue), white);
    }
}
