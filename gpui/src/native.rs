//! GPUI elements (macOS windows; the same walker also compiles on Linux). Rendering never invokes a kernel or waits for a composition.
use std::{collections::BTreeMap, time::Duration};

use gpui::{
    div, prelude::*, px, rgb, transparent_black, App, Context, Entity, Focusable, Hsla,
    Subscription, Window, WindowAppearance,
};
use ikigai_ui::model::{Node, Value};

use crate::{
    text,
    theme::{Theme, Tokens},
    worker::{Feedback, Pending, Worker},
    Snapshot, DISCLOSURE,
};
#[path = "input.rs"]
mod input;
use input::TextInput;

/// The palette tokens the native face draws with, by the role the HTML face gives each.
/// Every foreground/ground pair drawn from these is one `ikigai_ui::palette::PAIRS` tests.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    /// The window's ground.
    pub bg: Hsla,
    /// The unfilled meter track.
    pub surface: Hsla,
    /// A suboptimal meter reading on surface.
    pub warn: Hsla,
    /// Text on `bg`.
    pub fg: Hsla,
    /// Placeholder text on `bg`.
    pub muted: Hsla,
    /// Links on `bg`; the ground of buttons, selected options and selected text.
    pub accent: Hsla,
    /// Text on `accent`.
    pub accent_fg: Hsla,
    /// Field errors on `bg`.
    pub danger: Hsla,
    /// A control's border (inputs, options, forms) on `bg`.
    pub line: Hsla,
    /// A decorative divider (table rows) on `bg`.
    pub rule: Hsla,
    /// The focus ring and the text cursor, on `bg`.
    pub focus: Hsla,
}

impl Colors {
    /// The colors of one palette.
    pub fn new(tokens: &Tokens) -> Self {
        let c = |token| Hsla::from(rgb(tokens.get(token)));
        Self {
            bg: c("bg"),
            surface: c("surface"),
            warn: c("warn"),
            fg: c("fg"),
            muted: c("muted"),
            accent: c("accent"),
            accent_fg: c("accent-fg"),
            danger: c("danger"),
            line: c("line"),
            rule: c("rule"),
            focus: c("focus"),
        }
    }
}

/// A focus ring drawn outside its control, on the window's ground, as the HTML face's
/// `outline-offset` does: a transparent 2px border that takes `focus` when focused.
fn ring(
    child: impl IntoElement,
    focus: &gpui::FocusHandle,
    id: impl Into<gpui::ElementId>,
    colors: Colors,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .track_focus(focus)
        .p(px(2.))
        .border_2()
        .rounded_md()
        .border_color(transparent_black())
        .focus(move |s| s.border_color(colors.focus))
        .child(child)
}

fn dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// A `scene` drawn natively (ledger #3516): its datum (read on the worker), the window's camera
/// and selection, and the last frame painted (off the UI thread, by [`crate::scene::Painter`]).
#[cfg(feature = "scene")]
struct SceneState {
    loading: Option<Pending<ikigai_ui::scene::Scene>>,
    scene: Option<std::sync::Arc<ikigai_ui::scene::Scene>>,
    view: crate::scene::View,
    frame: Option<std::sync::Arc<gpui::RenderImage>>,
    painting: Option<Pending<Vec<u8>>>,
    /// The view or colors changed while a frame was painting: paint again when it lands.
    stale: bool,
    error: Option<String>,
}

/// The size of a scene's frame, in pixels (the html face's canvas is 16:9 too).
#[cfg(feature = "scene")]
pub const SCENE_FRAME: (u32, u32) = (640, 360);

struct FormState {
    feedback: Feedback,
    pending: Option<Pending<()>>,
}

/// A native screen window, with local control state separate from its immutable model.
pub struct Screen {
    worker: Worker,
    snapshot: Option<Snapshot>,
    loading: Option<Pending<Snapshot>>,
    message: String,
    inputs: BTreeMap<String, Entity<TextInput>>,
    choices: BTreeMap<String, String>,
    forms: BTreeMap<String, FormState>,
    focus: BTreeMap<String, gpui::FocusHandle>,
    /// Every focusable control by its node path, in document order. Tab moves through the
    /// ones shown ([`Self::shown`]): a closed disclosure's body is out of the order.
    tab_order: Vec<(String, gpui::FocusHandle)>,
    /// Each disclosure's open state, by node path: the window's, not the model's. The model
    /// carries only the initial state (`open`), read once when the snapshot is installed.
    open: BTreeMap<String, bool>,
    /// The screen's own focus, held when no control is focused (a window just opened, or a
    /// screen just loaded and took the focused control with it), so Tab still reaches the
    /// first control from the keyboard.
    root_focus: gpui::FocusHandle,
    /// Take `root_focus` at the next draw (a screen was loaded).
    refocus: bool,
    theme: Theme,
    dark: bool,
    /// The palette drawn with now, and the colors read from it.
    palette: Tokens,
    colors: Colors,
    /// Palettes already read through the kernel, by name.
    palettes: BTreeMap<String, Tokens>,
    palette_loading: Option<(String, Pending<Tokens>)>,
    /// Each natively drawn scene, by node path.
    #[cfg(feature = "scene")]
    scenes: BTreeMap<String, SceneState>,
    /// The painting thread, started by the first scene.
    #[cfg(feature = "scene")]
    painter: Option<crate::scene::Painter>,
    _appearance: Subscription,
}

impl Screen {
    /// Start loading without blocking the window. Polling is driven by a foreground
    /// timer; all composition and Sink calls run on the worker's OS threads.
    ///
    /// The window draws with `theme`'s palette, read as `urn:iki:ui:palette:{name}` on the
    /// worker; until it arrives (or if the kernel refuses it) the compiled table of the same
    /// name stands in. [`Theme::Auto`] follows the window's appearance as it changes.
    pub fn new(
        worker: Worker,
        screen: &str,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        input::bind_keys(cx);
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            this.set_appearance(window.appearance(), cx)
        });
        let dark = dark(window.appearance());
        let name = theme.palette(dark);
        let palette = Tokens::compiled(
            ikigai_ui::palette::palette(name).expect("a theme names a compiled palette"),
        );
        let colors = Colors::new(&palette);
        let palette_loading = Some((name.to_string(), worker.palette(name)));
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                break;
            }
        })
        .detach();
        let loading = Some(worker.load(screen));
        let root_focus = cx.focus_handle();
        window.focus(&root_focus);
        Self {
            worker,
            snapshot: None,
            loading,
            message: "Loading…".into(),
            inputs: BTreeMap::new(),
            choices: BTreeMap::new(),
            forms: BTreeMap::new(),
            focus: BTreeMap::new(),
            tab_order: Vec::new(),
            open: BTreeMap::new(),
            root_focus,
            refocus: false,
            theme,
            dark,
            palette,
            colors,
            palettes: BTreeMap::new(),
            palette_loading,
            #[cfg(feature = "scene")]
            scenes: BTreeMap::new(),
            #[cfg(feature = "scene")]
            painter: None,
            _appearance: appearance,
        }
    }

    /// The colors this window draws with now.
    pub fn colors(&self) -> Colors {
        self.colors
    }

    /// The name of the palette this window draws with now.
    pub fn palette(&self) -> &str {
        &self.palette.palette
    }

    /// Choose another theme; the window repaints when its palette is read.
    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.choose_palette(cx);
    }

    /// The window's system appearance changed (the window's observer calls this).
    pub fn set_appearance(&mut self, appearance: WindowAppearance, cx: &mut Context<Self>) {
        self.dark = dark(appearance);
        self.choose_palette(cx);
    }

    fn choose_palette(&mut self, cx: &mut Context<Self>) {
        let name = self.theme.palette(self.dark);
        if let Some(tokens) = self.palettes.get(name).cloned() {
            self.palette_loading = None;
            self.apply(tokens, cx);
        } else if self.palette_loading.as_ref().map(|(n, _)| n.as_str()) != Some(name) {
            self.palette_loading = Some((name.to_string(), self.worker.palette(name)));
        }
    }

    fn apply(&mut self, tokens: Tokens, cx: &mut Context<Self>) {
        self.colors = Colors::new(&tokens);
        self.palette = tokens;
        let colors = self.colors;
        for input in self.inputs.values() {
            input.update(cx, |input, cx| {
                input.colors = colors;
                cx.notify();
            });
        }
        #[cfg(feature = "scene")]
        for path in self.scenes.keys().cloned().collect::<Vec<_>>() {
            self.paint(&path);
        }
        cx.notify();
    }

    fn load(&mut self, name: &str, cx: &mut Context<Self>) {
        self.loading = Some(self.worker.load(name));
        self.message = "Loading…".into();
        cx.notify();
    }

    /// The snapshot this window draws now, once one has been read.
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    /// What the window says now: "Loading…", a refusal, or nothing.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Whether the screen and its palette have both been read (nothing is in flight).
    pub fn settled(&self) -> bool {
        self.loading.is_none() && self.palette_loading.is_none()
    }

    /// Take what the worker has finished. The window's timer calls this every frame; a test
    /// on GPUI's test platform calls it to settle a screen.
    pub fn poll(&mut self, cx: &mut Context<Self>) {
        if let Some((name, result)) = self
            .palette_loading
            .as_ref()
            .and_then(|(name, p)| Some((name.clone(), p.poll()?)))
        {
            self.palette_loading = None;
            match result {
                Ok(tokens) => {
                    self.palettes.insert(name, tokens.clone());
                    self.apply(tokens, cx);
                }
                Err(error) => {
                    // Keep the compiled table of the same name, and say why.
                    let tokens = Tokens::compiled(
                        ikigai_ui::palette::palette(&name).expect("a theme names a palette"),
                    );
                    self.palettes.insert(name, tokens.clone());
                    self.apply(tokens, cx);
                    self.message = error.to_string();
                }
            }
        }
        if let Some(result) = self.loading.as_ref().and_then(Pending::poll) {
            self.loading = None;
            match result {
                Ok(snapshot) => {
                    self.inputs.clear();
                    self.choices.clear();
                    self.forms.clear();
                    self.focus.clear();
                    self.tab_order.clear();
                    self.open.clear();
                    #[cfg(feature = "scene")]
                    self.scenes.clear();
                    self.install(snapshot.root(), "/", cx);
                    self.snapshot = Some(snapshot);
                    self.message.clear();
                    self.refocus = true;
                }
                Err(error) => self.message = error.to_string(),
            }
            cx.notify();
        }
        let mut changed = false;
        for form in self.forms.values_mut() {
            if let Some(result) = form.pending.as_ref().and_then(Pending::poll) {
                form.pending = None;
                form.feedback.complete(result);
                changed = true;
            }
        }
        #[cfg(feature = "scene")]
        {
            changed |= self.poll_scenes();
        }
        if changed {
            cx.notify();
        }
    }

    /// Land read datums and painted frames; paint again what went stale meanwhile.
    #[cfg(feature = "scene")]
    fn poll_scenes(&mut self) -> bool {
        let mut changed = false;
        for path in self.scenes.keys().cloned().collect::<Vec<_>>() {
            let state = self.scenes.get_mut(&path).expect("listed above");
            if let Some(result) = state.loading.as_ref().and_then(Pending::poll) {
                state.loading = None;
                changed = true;
                match result {
                    Ok(scene) => {
                        state.view = crate::scene::View::new(&scene);
                        state.scene = Some(std::sync::Arc::new(scene));
                        self.paint(&path);
                    }
                    Err(error) => state.error = Some(error.to_string()),
                }
            }
            let state = self.scenes.get_mut(&path).expect("listed above");
            if let Some(result) = state.painting.as_ref().and_then(Pending::poll) {
                state.painting = None;
                changed = true;
                match result.and_then(|bgra| frame_image(bgra, SCENE_FRAME)) {
                    Ok(image) => {
                        state.frame = Some(image);
                        state.error = None;
                    }
                    Err(error) => state.error = Some(error.to_string()),
                }
                if std::mem::take(&mut state.stale) {
                    self.paint(&path);
                }
            }
        }
        changed
    }

    /// Ask the painter for a frame of the scene at `path`, or mark it stale if one is painting.
    #[cfg(feature = "scene")]
    fn paint(&mut self, path: &str) {
        let colors = crate::scene::SceneColors::from_tokens(&self.palette);
        let painter = self.painter.get_or_insert_with(crate::scene::Painter::new);
        let Some(state) = self.scenes.get_mut(path) else {
            return;
        };
        let Some(scene) = state.scene.clone() else {
            return;
        };
        if state.painting.is_some() {
            state.stale = true;
            return;
        }
        let (width, height) = SCENE_FRAME;
        state.painting = Some(painter.paint(scene, state.view, colors, width, height));
    }

    /// A key on a focused scene: the html renderer's keys. The selection is announced in the
    /// window's status line, as the html face says it in `#status`.
    #[cfg(feature = "scene")]
    fn scene_key(
        &mut self,
        path: &str,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.scenes.get_mut(path) else {
            return false;
        };
        let Some(scene) = state.scene.clone() else {
            return false;
        };
        let key = keystroke.key.to_ascii_lowercase();
        let shift = keystroke.modifiers.shift;
        let key = match (key.as_str(), keystroke.key_char.as_deref()) {
            (_, Some("+")) => "+",
            (k, _) => k,
        };
        if !state.view.key(key, shift, scene.nodes.len()) {
            return false;
        }
        if ["n", "escape"].contains(&key) {
            self.message = crate::scene::announce(&scene, state.view.selected);
        }
        self.paint(path);
        cx.notify();
        true
    }

    /// The announced status line: what the window says now (loading, errors, a scene's
    /// selection).
    pub fn status(&self) -> &str {
        &self.message
    }

    fn install(&mut self, node: &Node, path: &str, cx: &mut Context<Self>) {
        match node.kind.as_str() {
            "field" => {
                let value = text(node, "value").unwrap_or_else(|| {
                    if text(node, "input") == Some("checkbox") {
                        "false"
                    } else {
                        ""
                    }
                });
                let colors = self.colors;
                let input = cx.new(|cx| {
                    let mut input = TextInput::new(value, colors, cx);
                    input.multiline = text(node, "input") == Some("textarea");
                    input
                });
                let focus = input.focus_handle(cx);
                self.tab_order.push((path.into(), focus.clone()));
                self.focus.insert(path.into(), focus);
                self.inputs.insert(path.into(), input);
            }
            "choice" => {
                self.choices.insert(
                    path.into(),
                    text(node, "value").unwrap_or_default().to_string(),
                );
                // A radio group (ledger #3537) is one tab stop: the group, not each option.
                if is_radio(node) {
                    let focus = cx.focus_handle();
                    self.tab_order.push((path.into(), focus.clone()));
                    self.focus.insert(path.into(), focus);
                }
            }
            "form" => {
                self.forms.insert(
                    path.into(),
                    FormState {
                        feedback: Feedback::default(),
                        pending: None,
                    },
                );
            }
            DISCLOSURE => {
                self.open.insert(
                    path.into(),
                    node.properties.get("open") == Some(&Value::Bool(true)),
                );
            }
            _ => {}
        }
        #[cfg(feature = "scene")]
        if node.kind == "scene" {
            let datum = text(node, "datum").unwrap_or_default();
            let state = SceneState {
                loading: Some(self.worker.scene(datum)),
                scene: None,
                view: crate::scene::View {
                    yaw: 0.6,
                    pitch: 0.3,
                    distance: 3.0,
                    selected: None,
                },
                frame: None,
                painting: None,
                stale: false,
                error: None,
            };
            self.scenes.insert(path.into(), state);
        }
        let in_radio = node.kind == "option"
            && path.rsplit_once('/').is_some_and(|(parent, _)| {
                self.focus.contains_key(parent) && self.choices.contains_key(parent)
            });
        if ["button", "link", "option", "scene", DISCLOSURE].contains(&node.kind.as_str())
            && !in_radio
        {
            let focus = cx.focus_handle();
            self.tab_order.push((path.into(), focus.clone()));
            self.focus.insert(path.into(), focus);
        }
        for (index, child) in node.children.iter().enumerate() {
            self.install(child, &format!("{path}/{index}"), cx);
        }
        // A scene's replacement (its accessible form) is drawn beside the image.
        if let Some(replacement) = node.replacement.as_deref() {
            self.install(replacement, &format!("{path}/replacement"), cx);
        }
    }

    /// Choose the `index`th option of the radio group `choice` at `path`, and say so in the
    /// status line as a native radio group is announced: its label, the option, checked, and
    /// where it is in the group.
    fn choose(&mut self, choice: &Node, path: &str, index: usize, cx: &mut Context<Self>) {
        let Some(option) = choice.children.get(index) else {
            return;
        };
        let value = text(option, "value").unwrap_or_default().to_string();
        self.choices.insert(path.into(), value);
        self.message = radio_announcement(choice, index);
        cx.notify();
    }

    /// Move the radio group at `path` by an arrow key, wrapping as native radios do: up and left
    /// to the previous option, down and right to the next. Whether the key was an arrow.
    fn radio_key(&mut self, choice: &Node, path: &str, key: &str, cx: &mut Context<Self>) -> bool {
        let len = choice.children.len();
        if len == 0 {
            return false;
        }
        let current = choice
            .children
            .iter()
            .position(|o| text(o, "value") == Some(self.choices[path].as_str()));
        let next = match (key, current) {
            ("up" | "left", Some(i)) => (i + len - 1) % len,
            ("down" | "right", Some(i)) => (i + 1) % len,
            ("up" | "left", None) => len - 1,
            ("down" | "right", None) => 0,
            ("space", Some(i)) => i,
            ("space", None) => 0,
            _ => return false,
        };
        self.choose(choice, path, next, cx);
        true
    }

    /// Whether the node at `path` is shown: no disclosure above it is closed.
    fn shown(&self, path: &str) -> bool {
        !self.open.iter().any(|(at, open)| {
            !open
                && path.len() > at.len()
                && path.starts_with(at.as_str())
                && path[at.len()..].starts_with('/')
        })
    }

    /// Open a closed disclosure, close an open one.
    fn toggle(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(open) = self.open.get_mut(path) {
            *open = !*open;
            cx.notify();
        }
    }

    fn values(&self, node: &Node, path: &str, cx: &App, values: &mut BTreeMap<String, String>) {
        if let Some(name) = text(node, "name") {
            match node.kind.as_str() {
                "field" => {
                    values.insert(name.into(), self.inputs[path].read(cx).content.to_string());
                }
                "choice" => {
                    values.insert(name.into(), self.choices[path].clone());
                }
                _ => {}
            }
        }
        for (i, child) in node.children.iter().enumerate() {
            if child.kind != "form" {
                self.values(child, &format!("{path}/{i}"), cx, values);
            }
        }
    }

    fn submit(&mut self, form: &Node, path: &str, button: &Node, cx: &mut Context<Self>) {
        if self.forms[path].pending.is_some() {
            return;
        }
        let mut values = BTreeMap::new();
        self.values(form, path, cx, &mut values);
        if let (Some(name), Some(value)) = (text(button, "name"), text(button, "value")) {
            values.insert(name.into(), value.into());
        }
        let pending = self.worker.submit(form, values);
        let state = self.forms.get_mut(path).unwrap();
        state.feedback = Feedback::default();
        state.pending = Some(pending);
        cx.notify();
    }

    /// Follow a link: `/page/{name}` loads the screen `{name}` through the host's kernel (the
    /// names the HTML face routes), so any app's screens navigate natively. A name that is not
    /// a screen name is refused here; one the kernel has no screen for is refused by the worker,
    /// and either way the window keeps the screen it shows and says why. `https:`, `http:` and
    /// `mailto:` open in the system's handler; anything else is refused.
    pub fn follow(&mut self, href: &str, cx: &mut Context<Self>) {
        if let Some(name) = href.strip_prefix("/page/") {
            if !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                self.load(name, cx);
            } else {
                self.message = format!("No native route for {href}");
                cx.notify();
            }
        } else if href.starts_with("https://")
            || href.starts_with("http://")
            || href.starts_with("mailto:")
        {
            cx.open_url(href);
        } else {
            self.message = format!("No native route for {href}");
            cx.notify();
        }
    }

    fn walk(
        &self,
        node: &Node,
        path: &str,
        form: Option<(&Node, &str)>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let label = || text(node, "label").unwrap_or_default().to_string();
        let colors = self.colors;
        let mut body = div()
            .id(gpui::SharedString::from(path.to_string()))
            .flex()
            .flex_col()
            .gap_2()
            .w_full();
        match node.kind.as_str() {
            "heading" => {
                let level = match node.properties.get("level") {
                    Some(Value::Number(n)) => *n,
                    _ => 2,
                };
                return body
                    .text_size(px((30 - level * 2) as f32))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(text(node, "content").unwrap_or_default().to_string())
                    .into_any_element();
            }
            "text" | "status" => {
                return body
                    .child(text(node, "content").unwrap_or_default().to_string())
                    .into_any_element()
            }
            "field" => {
                let input = self.inputs[path].clone();
                let focus = input.focus_handle(cx);
                if text(node, "input") == Some("checkbox") {
                    let checked = input.read(cx).content.as_ref() == "true";
                    let key_input = input.clone();
                    body = body
                        .track_focus(&focus)
                        .p(px(2.))
                        .border_2()
                        .rounded_md()
                        .border_color(transparent_black())
                        .focus(move |s| s.border_color(colors.focus))
                        .child(format!("{} {}", if checked { "☑" } else { "☐" }, label()))
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, window, cx| {
                            window.focus(&focus);
                            input.update(cx, |input, cx| {
                                input.content = if checked { "false" } else { "true" }.into();
                                cx.notify();
                            });
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(move |_, event: &gpui::KeyDownEvent, _, cx| {
                            if event.keystroke.key == "space" {
                                key_input.update(cx, |input, cx| {
                                    input.content = if checked { "false" } else { "true" }.into();
                                    cx.notify();
                                });
                                cx.notify();
                            }
                        }));
                } else {
                    let required =
                        matches!(node.properties.get("required"), Some(Value::Bool(true)))
                            || text(node, "required") == Some("true");
                    body = body
                        .child(
                            div()
                                .id(gpui::SharedString::from(format!("label:{path}")))
                                .child(format!("{}{}", label(), if required { " *" } else { "" }))
                                .cursor_pointer()
                                .on_click(move |_, window, _| window.focus(&focus)),
                        )
                        .child(input);
                }
            }
            "choice" if is_radio(node) => {
                // A radio group (ledger #3537): one focusable group whose arrows move the
                // selection, edged by `line` with its label as the legend; the chosen option is
                // filled (●) and the others empty (○), and the ring is the group's.
                let focus = self.focus[path].clone();
                let mut group = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child(label()));
                for (i, option) in node.children.iter().enumerate() {
                    let selected = text(option, "value") == Some(self.choices[path].as_str());
                    let (click_node, click_path, click_focus) =
                        (node.clone(), path.to_string(), focus.clone());
                    group = group.child(
                        div()
                            .id(gpui::SharedString::from(format!("{path}/{i}")))
                            .flex()
                            .gap_2()
                            .min_h(px(24.))
                            .cursor_pointer()
                            .child(if selected { "●" } else { "○" })
                            .child(text(option, "label").unwrap_or_default().to_string())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&click_focus);
                                this.choose(&click_node, &click_path, i, cx);
                            })),
                    );
                }
                let (key_node, key_path) = (node.clone(), path.to_string());
                body = body.child(
                    ring(
                        div()
                            .p_2()
                            .border_1()
                            .rounded_md()
                            .border_color(colors.line)
                            .child(group),
                        &focus,
                        gpui::SharedString::from(format!("{path}:radios")),
                        colors,
                    )
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, _, cx| {
                            if this.radio_key(&key_node, &key_path, &event.keystroke.key, cx) {
                                cx.stop_propagation();
                            }
                        },
                    )),
                );
            }
            "choice" => {
                body = body.child(label());
                for (i, option) in node.children.iter().enumerate() {
                    let value = text(option, "value").unwrap_or_default().to_string();
                    let selected = self.choices[path] == value;
                    let focus = self.focus[&format!("{path}/{i}")].clone();
                    let path = path.to_string();
                    let key_path = path.clone();
                    let key_value = value.clone();
                    // A selected option is accent-fg on accent, edged by its own ground; an
                    // unselected one is fg on bg, edged by line. The ring sits outside, on bg.
                    let (ground, ink, edge) = if selected {
                        (colors.accent, colors.accent_fg, colors.accent)
                    } else {
                        (colors.bg, colors.fg, colors.line)
                    };
                    let option_view = div()
                        .p_2()
                        .border_1()
                        .rounded_md()
                        .border_color(edge)
                        .bg(ground)
                        .text_color(ink)
                        .child(format!(
                            "{} {}",
                            if selected { "●" } else { "○" },
                            text(option, "label").unwrap_or_default()
                        ));
                    body = body.child(
                        ring(option_view, &focus, i, colors)
                            .cursor_pointer()
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.key == "space"
                                        || event.keystroke.key == "enter"
                                    {
                                        this.choices.insert(key_path.clone(), key_value.clone());
                                        cx.notify();
                                    }
                                },
                            ))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&focus);
                                this.choices.insert(path.clone(), value.clone());
                                cx.notify();
                            })),
                    );
                }
            }
            "button" => {
                let focus = self.focus[path].clone();
                let mut caption = div()
                    .p_2()
                    .rounded_md()
                    .bg(colors.accent)
                    .text_color(colors.accent_fg)
                    .child(label());
                let busy =
                    form.is_some_and(|(_, form_path)| self.forms[form_path].pending.is_some());
                if busy {
                    caption = caption.child(" (Submitting…)");
                }
                let mut button = ring(
                    caption,
                    &focus,
                    gpui::SharedString::from(path.to_string()),
                    colors,
                );
                if let Some((form, form_path)) = form {
                    if !busy {
                        let (form, form_path, node) =
                            (form.clone(), form_path.to_string(), node.clone());
                        let (key_form, key_path, key_node) =
                            (form.clone(), form_path.clone(), node.clone());
                        button = button
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, _, cx| {
                                    if event.keystroke.key == "space"
                                        || event.keystroke.key == "enter"
                                    {
                                        this.submit(&key_form, &key_path, &key_node, cx);
                                    }
                                },
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.submit(&form, &form_path, &node, cx)
                            }));
                    }
                }
                return button.into_any_element();
            }
            "link" => {
                let href = text(node, "href").unwrap_or_default().to_string();
                let key_href = href.clone();
                let focus = self.focus[path].clone();
                return body
                    .track_focus(&focus)
                    .child(label())
                    .text_color(colors.accent)
                    .cursor_pointer()
                    .p(px(2.))
                    .border_2()
                    .rounded_md()
                    .border_color(transparent_black())
                    .focus(move |s| s.underline().border_color(colors.focus))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            this.follow(&key_href, cx);
                        }
                    }))
                    .hover(|s| s.underline())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.follow(&href, cx);
                    }))
                    .into_any_element();
            }
            "option" => return body.child(label()).into_any_element(),
            #[cfg(feature = "scene")]
            "scene" => {
                // The image is only pixels: it takes focus and the keys, and the replacement
                // (the graph as text) is drawn beside it, always.
                let focus = self.focus[path].clone();
                let state = &self.scenes[path];
                let (width, height) = SCENE_FRAME;
                let picture = match (&state.frame, &state.error) {
                    (Some(frame), _) => gpui::img(gpui::ImageSource::Render(frame.clone()))
                        .w(px(width as f32))
                        .h(px(height as f32))
                        .into_any_element(),
                    (None, Some(error)) => div().child(error.clone()).into_any_element(),
                    (None, None) => div()
                        .w(px(width as f32))
                        .h(px(height as f32))
                        .bg(colors.bg)
                        .child("Drawing the scene…")
                        .into_any_element(),
                };
                let key_path = path.to_string();
                let click_focus = focus.clone();
                let canvas = ring(
                    picture,
                    &focus,
                    gpui::SharedString::from(format!("{path}:canvas")),
                    colors,
                )
                .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                    if this.scene_key(&key_path, &event.keystroke, cx) {
                        cx.stop_propagation();
                    }
                }))
                .on_click(move |_, window, _| window.focus(&click_focus));
                body = body.child(label()).child(canvas).child(
                    "Arrows orbit, + and - zoom, N and Shift+N select a node, Escape clears.",
                );
                if let Some(replacement) = node.replacement.as_deref() {
                    body = body.child(self.walk(
                        replacement,
                        &format!("{path}/replacement"),
                        form,
                        cx,
                    ));
                }
                return body.into_any_element();
            }
            ikigai_ui::meter::IRI => {
                let reading = match ikigai_ui::meter::Reading::parse(node) {
                    Ok(reading) => reading,
                    Err(error) => {
                        return body
                            .child(format!("{}: {error}", label()))
                            .into_any_element()
                    }
                };
                let fill = match reading.token() {
                    "danger" => colors.danger,
                    "warn" => colors.warn,
                    _ => colors.accent,
                };
                // The fill and the border meet surface, never one another: a 2px inset
                // keeps every drawn pair in PAIRS. Empty/full readings keep the same border.
                let bar = div()
                    .w(px(164.))
                    .h(px(20.))
                    .flex_shrink_0()
                    .p(px(2.))
                    .border_1()
                    .border_color(colors.line)
                    .bg(colors.surface)
                    .child(div().h_full().w(px(158. * reading.fraction())).bg(fill));
                return body
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(bar)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(200.))
                            .child(reading.description(node)),
                    )
                    .into_any_element();
            }
            DISCLOSURE => {
                // A button that holds its expanded state (`open`, the window's) and shows or
                // hides its body: the summary line, then the body text and children when open.
                let open = self.open[path];
                let focus = self.focus[path].clone();
                let summary = text(node, "summary").unwrap_or_default().to_string();
                let key_path = path.to_string();
                let click_path = path.to_string();
                let control = ring(
                    div()
                        .flex()
                        .gap_2()
                        .p_1()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(if open { "▾" } else { "▸" })
                        .child(summary),
                    &focus,
                    gpui::SharedString::from(format!("{path}:summary")),
                    colors,
                )
                .cursor_pointer()
                .hover(|s| s.underline())
                .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "space" || event.keystroke.key == "enter" {
                        this.toggle(&key_path, cx);
                        cx.stop_propagation();
                    }
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle(&click_path, cx)));
                body = body.child(control);
                if open {
                    let mut inner = div().flex().flex_col().gap_2().pl_6();
                    if let Some(text) = text(node, "body") {
                        inner = inner.child(text.to_string());
                    }
                    for (i, child) in node.children.iter().enumerate() {
                        inner = inner.child(self.walk(child, &format!("{path}/{i}"), form, cx));
                    }
                    body = body.child(inner);
                }
                return body.into_any_element();
            }
            "form" | "group" | "list" | "nav" | "table" => {
                body = body.child(if node.kind == "table" {
                    text(node, "caption").unwrap_or_default().to_string()
                } else {
                    label()
                });
                if node.kind == "form" {
                    body = body.p_4().border_1().border_color(colors.line).rounded_md();
                }
                let form = if node.kind == "form" {
                    Some((node, path))
                } else {
                    form
                };
                if let Some(description) = text(node, "description") {
                    body = body.child(description.to_string());
                }
                for (i, child) in node.children.iter().enumerate() {
                    if node.kind == "table" {
                        // A row's group children are laid out as native cells, not markup.
                        let mut row = div()
                            .flex()
                            .flex_row()
                            .gap_4()
                            .border_b_1()
                            .border_color(colors.rule);
                        for (j, cell) in child.children.iter().enumerate() {
                            let mut cell_view = div().flex_1().child(self.walk(
                                cell,
                                &format!("{path}/{i}/{j}"),
                                form,
                                cx,
                            ));
                            if text(child, "header") == Some("column")
                                || (j == 0 && text(child, "header") == Some("row"))
                            {
                                cell_view = cell_view.font_weight(gpui::FontWeight::BOLD);
                            }
                            row = row.child(cell_view);
                        }
                        body = body.child(row);
                    } else {
                        let element = self.walk(child, &format!("{path}/{i}"), form, cx);
                        if node.kind == "list" {
                            body = body.child(div().flex().gap_2().child("•").child(element));
                        } else {
                            body = body.child(element);
                        }
                    }
                }
                if node.kind == "form" {
                    let state = &self.forms[path];
                    body = body.child(state.feedback.message.clone());
                }
                return body.into_any_element();
            }
            _ => unreachable!("Snapshot rejects unsupported kinds"),
        }
        if let Some(description) = text(node, "description") {
            body = body.child(description.to_string());
        }
        let error = form
            .and_then(|(_, path)| self.forms[path].feedback.fields.get(text(node, "name")?))
            .map(String::as_str)
            .or_else(|| text(node, "error"));
        if let Some(error) = error {
            body = body.child(div().text_color(colors.danger).child(error.to_string()));
        }
        body.into_any_element()
    }
}

/// Whether `node` is a choice its screen asks to show as radio buttons, the hint kept only
/// because this face's descriptor claims `urn:iki:ui:hint:choice:` (ledger #3537).
fn is_radio(node: &Node) -> bool {
    ikigai_ui::model::is_radio(node)
}

/// What a radio group says when its `index`th option is chosen: `Lens: contact-table,
/// checked, 2 of 4`.
pub fn radio_announcement(choice: &Node, index: usize) -> String {
    let option = choice
        .children
        .get(index)
        .and_then(|o| text(o, "label"))
        .unwrap_or_default();
    format!(
        "{}: {option}, checked, {} of {}",
        text(choice, "label").unwrap_or_default(),
        index + 1,
        choice.children.len()
    )
}

/// A painted frame as GPUI's image: BGRA already, as [`gpui::RenderImage`] keeps its pixels.
#[cfg(feature = "scene")]
fn frame_image(
    bgra: Vec<u8>,
    (width, height): (u32, u32),
) -> Result<std::sync::Arc<gpui::RenderImage>, ikigai_core::Error> {
    let buffer = image::RgbaImage::from_raw(width, height, bgra)
        .ok_or_else(|| ikigai_core::Error::Endpoint("a frame of the wrong size".into()))?;
    Ok(std::sync::Arc::new(gpui::RenderImage::new(vec![
        image::Frame::new(buffer),
    ])))
}

impl Render for Screen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.refocus)
            && !self.tab_order.iter().any(|(_, f)| f.is_focused(window))
        {
            window.focus(&self.root_focus);
        }
        let mut body = div()
            .id("screen")
            .track_focus(&self.root_focus)
            .size_full()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let order: Vec<gpui::FocusHandle> = this
                    .tab_order
                    .iter()
                    .filter(|(path, _)| this.shown(path))
                    .map(|(_, focus)| focus.clone())
                    .collect();
                if event.keystroke.key == "tab" && !order.is_empty() {
                    let current = order.iter().position(|focus| focus.is_focused(window));
                    let len = order.len();
                    let next = match (current, event.keystroke.modifiers.shift) {
                        (Some(i), true) => (i + len - 1) % len,
                        (Some(i), false) => (i + 1) % len,
                        (None, true) => len - 1,
                        (None, false) => 0,
                    };
                    window.focus(&order[next]);
                    cx.stop_propagation();
                }
            }))
            .bg(self.colors.bg)
            .text_color(self.colors.fg)
            .text_size(px(16.))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .id("gallery")
                            .cursor_pointer()
                            .child("Gallery")
                            .on_click(cx.listener(|this, _, _, cx| this.load("gallery", cx))),
                    )
                    .child(
                        div()
                            .id("contact")
                            .cursor_pointer()
                            .child("Contact")
                            .on_click(cx.listener(|this, _, _, cx| this.load("contact", cx))),
                    ),
            )
            .child(self.message.clone());
        if let Some(snapshot) = self.snapshot.clone() {
            body = body.child(self.walk(snapshot.root(), "/", None, cx));
        }
        body
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use ikigai_core::{
        Capability, Description, EndpointSpace, Exact, Fallback, FnEndpoint, Invocation, ReprType,
        Representation, Scope, Verb,
    };
    use std::{
        sync::{mpsc, Arc},
        time::Instant,
    };

    fn demo_worker(extra: Option<EndpointSpace>) -> Worker {
        let mut spaces: Vec<Arc<dyn ikigai_core::Space>> = Vec::new();
        if let Some(extra) = extra {
            spaces.push(Arc::new(extra));
        }
        spaces.push(Arc::new(crate::descriptor_space()));
        spaces.push(ikigai_ui_demo::root(Default::default()));
        Worker::new(
            Arc::new(ikigai_ui_demo::kernel(Arc::new(Fallback::new(spaces)))),
            Capability::root(),
            Scope::empty(),
        )
    }

    /// Poll until the screen and its palette have both been read.
    fn settle(screen: &Entity<Screen>, cx: &mut gpui::VisualTestContext) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !screen.update(cx, |s, cx| {
            s.poll(cx);
            s.loading.is_none() && s.palette_loading.is_none()
        }) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        cx.run_until_parked();
    }

    fn painted(palette: &ikigai_ui::palette::Palette) -> Colors {
        Colors::new(&Tokens::compiled(palette))
    }

    #[gpui::test]
    fn each_theme_paints_its_palettes_ground_and_text(cx: &mut TestAppContext) {
        use ikigai_ui::palette::{DARK, HIGH_CONTRAST, LIGHT};
        for (theme, palette) in [
            (Theme::Light, LIGHT),
            (Theme::Dark, DARK),
            (Theme::HighContrast, HIGH_CONTRAST),
        ] {
            let worker = demo_worker(None);
            let (screen, cx) =
                cx.add_window_view(|window, cx| Screen::new(worker, "gallery", theme, window, cx));
            settle(&screen, cx);
            screen.update(cx, |s, _| {
                assert!(s.snapshot.is_some(), "{}", s.message);
                assert_eq!(s.palette(), palette.name);
                assert_eq!(s.colors(), painted(&palette), "{}", palette.name);
                let bg = Hsla::from(rgb(u32::from_str_radix(
                    &palette.get("bg").unwrap()[1..],
                    16,
                )
                .unwrap()));
                let fg = Hsla::from(rgb(u32::from_str_radix(
                    &palette.get("fg").unwrap()[1..],
                    16,
                )
                .unwrap()));
                assert_eq!((s.colors().bg, s.colors().fg), (bg, fg), "{}", palette.name);
            });
        }
    }

    #[gpui::test]
    fn a_hosts_palette_override_reaches_the_native_face(cx: &mut TestAppContext) {
        use ikigai_ui::palette::{DARK, HIGH_CONTRAST};
        // The host repaints the dark palette's ground; nothing else changes.
        let overridden = DARK
            .declarations()
            .replace(DARK.get("bg").unwrap(), HIGH_CONTRAST.get("bg").unwrap());
        let served = overridden.clone();
        let host = EndpointSpace::new().bind(
            Exact::new("urn:iki:ui:palette:dark"),
            FnEndpoint::new("host-dark", move |_: &Invocation<'_>| {
                Ok(Representation::new(
                    ReprType::new("text/plain"),
                    served.clone().into_bytes(),
                ))
            }),
        );
        let worker = demo_worker(Some(host));
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "gallery", Theme::Dark, window, cx));
        settle(&screen, cx);
        let expected = Colors::new(&Tokens::parse("dark", &overridden).unwrap());
        screen.update(cx, |s, _| {
            assert_eq!(s.colors(), expected);
            assert_ne!(s.colors(), painted(&DARK));
        });
    }

    #[gpui::test]
    fn auto_follows_the_window_appearance_and_explicit_themes_hold(cx: &mut TestAppContext) {
        use ikigai_ui::palette::{DARK, HIGH_CONTRAST, LIGHT};
        let worker = demo_worker(None);
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "gallery", Theme::Auto, window, cx));
        settle(&screen, cx);
        // GPUI's test window reports a light appearance.
        screen.update(cx, |s, _| assert_eq!(s.colors(), painted(&LIGHT)));
        // The window's appearance observer calls set_appearance; the test platform cannot
        // change a window's appearance, so the test calls it as the observer does.
        for (appearance, palette) in [
            (WindowAppearance::Dark, DARK),
            (WindowAppearance::Light, LIGHT),
            (WindowAppearance::VibrantDark, DARK),
            (WindowAppearance::VibrantLight, LIGHT),
        ] {
            screen.update(cx, |s, cx| s.set_appearance(appearance, cx));
            settle(&screen, cx);
            screen.update(cx, |s, _| {
                assert_eq!(s.palette(), palette.name, "{appearance:?}");
                assert_eq!(s.colors(), painted(&palette), "{appearance:?}");
            });
        }
        // An explicit theme does not follow the appearance.
        screen.update(cx, |s, cx| s.set_theme(Theme::HighContrast, cx));
        settle(&screen, cx);
        for appearance in [WindowAppearance::Light, WindowAppearance::Dark] {
            screen.update(cx, |s, cx| s.set_appearance(appearance, cx));
            settle(&screen, cx);
            screen.update(cx, |s, _| assert_eq!(s.colors(), painted(&HIGH_CONTRAST)));
        }
        // Inputs installed earlier repaint with the window.
        screen.update(cx, |s, cx| {
            for input in s.inputs.values() {
                assert_eq!(input.read(cx).colors, painted(&HIGH_CONTRAST));
            }
        });
    }

    #[gpui::test]
    fn gallery_walks_every_native_kind_and_boolean_fields_toggle(cx: &mut TestAppContext) {
        let root = Arc::new(Fallback::new(vec![
            Arc::new(crate::descriptor_space()),
            ikigai_ui_demo::root(Default::default()),
        ]));
        let worker = Worker::new(
            Arc::new(ikigai_ui_demo::kernel(root)),
            Capability::root(),
            Scope::empty(),
        );
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "gallery", Theme::Auto, window, cx));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if screen.update(cx, |s, cx| {
                s.poll(cx);
                s.loading.is_none()
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        cx.run_until_parked(); // Draw the gallery, including tables, lists, links and options.
        screen.update(cx, |s, _| assert!(s.snapshot.is_some(), "{}", s.message));
        let snapshot = Snapshot::from_face(br#"{"kind":"field","properties":{"label":"Enabled","name":"enabled","input":"checkbox"}}"#).unwrap();
        let focus = screen.update(cx, |s, cx| {
            s.install(snapshot.root(), "/", cx);
            s.snapshot = Some(snapshot);
            let mut values = BTreeMap::new();
            s.values(s.snapshot.as_ref().unwrap().root(), "/", cx, &mut values);
            assert_eq!(values["enabled"], "false");
            cx.notify();
            s.focus["/"].clone()
        });
        cx.update(|window, _| window.focus(&focus));
        cx.simulate_keystrokes("space");
        screen.update(cx, |s, cx| {
            let mut values = BTreeMap::new();
            s.values(s.snapshot.as_ref().unwrap().root(), "/", cx, &mut values);
            assert_eq!(values["enabled"], "true");
        });
    }

    /// The GPUI descriptor with no element claimed: the native disclosure turned off.
    fn without_native_elements() -> EndpointSpace {
        let mut descriptor: serde_json::Value = serde_json::from_slice(crate::DESCRIPTOR).unwrap();
        descriptor.as_object_mut().unwrap().remove("elements");
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        EndpointSpace::new().bind(
            Exact::new("urn:iki:ui:target:name:gpui"),
            FnEndpoint::new("test-gpui-no-elements", move |_: &Invocation<'_>| {
                Ok(
                    Representation::new(ReprType::new("application/json"), bytes.clone())
                        .cacheable(),
                )
            })
            .with_description(
                Description::new("test-gpui-no-elements")
                    .title("GPUI without native elements")
                    .summary("The native descriptor, claiming no element.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output("application/json"),
            ),
        )
    }

    #[gpui::test]
    fn a_disclosure_opens_and_closes_in_the_window_and_its_body_joins_the_tab_order(
        cx: &mut TestAppContext,
    ) {
        let worker = demo_worker(None);
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "elements", Theme::Auto, window, cx));
        settle(&screen, cx);
        // [heading, rating, card, key-value, tag-list, disclosure]: the disclosure arrives
        // whole, closed as its model says, its body's link out of the tab order.
        let (summary, link, model) = screen.update(cx, |s, _| {
            let root = s.snapshot.as_ref().expect("the elements screen").root();
            assert_eq!(root.children[5].kind, DISCLOSURE);
            assert_eq!(s.open.get("//5"), Some(&false));
            (
                s.focus["//5"].clone(),
                s.focus["//5/0"].clone(),
                root.children[5].clone(),
            )
        });
        let order = |cx: &mut gpui::VisualTestContext| {
            screen.update(cx, |s, _| {
                s.tab_order
                    .iter()
                    .filter(|(path, _)| s.shown(path))
                    .map(|(path, _)| path.clone())
                    .collect::<Vec<_>>()
            })
        };
        let closed = order(cx);
        assert!(closed.contains(&"//5".to_string()) && !closed.contains(&"//5/0".to_string()));
        // Tab from the summary wraps past the hidden link.
        cx.update(|window, _| window.focus(&summary));
        cx.simulate_keystrokes("tab");
        cx.update(|window, _| assert!(!link.is_focused(window)));
        // Space on the summary opens it; the model is untouched (the state is the window's).
        cx.update(|window, _| window.focus(&summary));
        cx.simulate_keystrokes("space");
        screen.update(cx, |s, _| {
            assert!(s.open["//5"]);
            assert_eq!(s.snapshot.as_ref().unwrap().root().children[5], model);
        });
        assert!(order(cx).contains(&"//5/0".to_string()));
        // Open, the body's link is the next stop after the summary, by keyboard alone.
        cx.simulate_keystrokes("tab");
        cx.update(|window, _| assert!(link.is_focused(window)));
        // Enter on the summary closes it again.
        cx.update(|window, _| window.focus(&summary));
        cx.simulate_keystrokes("enter");
        screen.update(cx, |s, _| assert!(!s.open["//5"]));
        assert!(!order(cx).contains(&"//5/0".to_string()));
    }

    #[gpui::test]
    fn with_the_native_disclosure_off_the_window_draws_its_expansion_always_open(
        cx: &mut TestAppContext,
    ) {
        let worker = demo_worker(Some(without_native_elements()));
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "elements", Theme::Auto, window, cx));
        settle(&screen, cx);
        screen.update(cx, |s, _| {
            let root = s.snapshot.as_ref().expect("the elements screen").root();
            let expanded = &root.children[5];
            assert_eq!(expanded.kind, "group");
            assert_eq!(
                text(expanded, "label"),
                Some("How often is the gauge read?")
            );
            assert_eq!(expanded.children[0].kind, "heading");
            assert_eq!(expanded.children[1].kind, "text");
            assert_eq!(expanded.children[2].kind, "link");
            // Nothing to toggle, nothing hidden: its link is in the tab order.
            assert!(s.open.is_empty());
            assert!(s.tab_order.iter().any(|(path, _)| path == "//5/2"));
            assert!(s.shown("//5/2"));
        });
    }

    #[gpui::test]
    fn meters_draw_natively_in_each_palette_without_taking_keyboard_focus(cx: &mut TestAppContext) {
        for theme in [Theme::Light, Theme::Dark, Theme::HighContrast] {
            let worker = demo_worker(None);
            let (screen, cx) =
                cx.add_window_view(|window, cx| Screen::new(worker, "elements", theme, window, cx));
            settle(&screen, cx);
            screen.update(cx, |s, _| {
                let meter = &s.snapshot.as_ref().unwrap().root().children[6];
                assert_eq!(meter.kind, ikigai_ui::meter::IRI);
                let reading = ikigai_ui::meter::Reading::parse(meter).unwrap();
                assert!((reading.fraction() - 0.82).abs() < f32::EPSILON);
                assert_eq!(reading.token(), "warn");
                assert!(reading
                    .description(meter)
                    .contains("outside optimum region"));
                assert!(!s.focus.contains_key("//6"));
                assert!(!s.tab_order.iter().any(|(path, _)| path == "//6"));
                let expected = Colors::new(&Tokens::compiled(
                    ikigai_ui::palette::palette(theme.palette(false)).unwrap(),
                ));
                assert_eq!(s.colors.surface, expected.surface);
                assert_eq!(s.colors.warn, expected.warn);
            });
        }
    }

    #[gpui::test]
    fn meters_draw_their_text_expansion_when_native_support_is_disabled(cx: &mut TestAppContext) {
        let worker = demo_worker(Some(without_native_elements()));
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "elements", Theme::Auto, window, cx));
        settle(&screen, cx);
        screen.update(cx, |s, _| {
            let meter = &s.snapshot.as_ref().unwrap().root().children[6];
            assert_eq!(meter.kind, "text");
            assert_eq!(
                text(meter, "content"),
                Some("Reservoir capacity (%): 82 of 100")
            );
            assert!(!s.focus.contains_key("//6"));
        });
    }

    #[gpui::test]
    fn window_accepts_native_text_during_a_five_second_composition(cx: &mut TestAppContext) {
        let (started, signal) = mpsc::channel();
        let ui_thread = std::thread::current().id();
        let slow = EndpointSpace::new().bind(
            Exact::new("urn:iki:ui:screen:name:slow"),
            FnEndpoint::new("slow-window-composition", move |_: &Invocation<'_>| {
                assert_ne!(std::thread::current().id(), ui_thread);
                started.send(()).unwrap();
                std::thread::sleep(Duration::from_secs(5));
                Ok(Representation::new(
                    ReprType::new(ikigai_ui::model::MEDIA_TYPE),
                    br#"{"kind":"text","properties":{"content":"Resolved"}}"#.to_vec(),
                ))
            })
            .with_description(
                Description::new("slow-window-composition")
                    .title("Slow")
                    .summary("A blocking composition.")
                    .verb(Verb::Source)
                    .output(ikigai_ui::model::MEDIA_TYPE),
            ),
        );
        let root = Arc::new(Fallback::new(vec![
            Arc::new(slow),
            Arc::new(crate::descriptor_space()),
            ikigai_ui_demo::root(Default::default()),
        ]));
        let worker = Worker::new(
            Arc::new(ikigai_ui_demo::kernel(root)),
            Capability::root(),
            Scope::empty(),
        );
        let (screen, cx) =
            cx.add_window_view(|window, cx| Screen::new(worker, "slow", Theme::Auto, window, cx));
        signal.recv_timeout(Duration::from_secs(2)).unwrap();
        // An already visible input must remain usable while the new screen resolves.
        let snapshot = Snapshot::from_face(
            br#"{"kind":"field","properties":{"label":"Search","name":"search"}}"#,
        )
        .unwrap();
        let input = screen.update(cx, |screen, cx| {
            screen.install(snapshot.root(), "/", cx);
            screen.snapshot = Some(snapshot);
            screen.inputs["/"].clone()
        });
        cx.update(|window, cx| window.focus(&input.focus_handle(cx)));
        cx.run_until_parked();
        let before = Instant::now();
        cx.simulate_input("Still responsive 🦀");
        assert!(before.elapsed() < Duration::from_millis(500));
        cx.update(|_, cx| assert_eq!(input.read(cx).content.as_ref(), "Still responsive 🦀"));
        screen.update(cx, |screen, _| {
            assert!(screen.loading.as_ref().unwrap().poll().is_none())
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let ready = screen.update(cx, |screen, cx| {
                screen.poll(cx);
                screen.loading.is_none()
            });
            if ready {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        cx.update(|_, cx| {
            assert_eq!(
                text(screen.read(cx).snapshot.as_ref().unwrap().root(), "content"),
                Some("Resolved")
            )
        });
    }

    #[gpui::test]
    fn a_late_old_load_cannot_replace_the_newer_snapshot(cx: &mut TestAppContext) {
        let (started, start) = mpsc::channel();
        let (finished, finish) = mpsc::channel();
        let release = Arc::new(std::sync::Barrier::new(2));
        let gate = release.clone();
        let space = EndpointSpace::new()
            .bind(
                Exact::new("urn:iki:ui:face:target:gpui:screen:old"),
                FnEndpoint::new("old-face", move |_: &Invocation<'_>| {
                    started.send(()).unwrap();
                    gate.wait();
                    finished.send(()).unwrap();
                    Ok(Representation::new(
                        ReprType::new(ikigai_ui::model::MEDIA_TYPE),
                        br#"{"kind":"text","properties":{"content":"Old"}}"#.to_vec(),
                    ))
                }),
            )
            .bind(
                Exact::new("urn:iki:ui:face:target:gpui:screen:new"),
                FnEndpoint::new("new-face", |_: &Invocation<'_>| {
                    Ok(Representation::new(
                        ReprType::new(ikigai_ui::model::MEDIA_TYPE),
                        br#"{"kind":"text","properties":{"content":"New"}}"#.to_vec(),
                    ))
                }),
            );
        let worker = Worker::new(
            Arc::new(ikigai_core::Kernel::new(Arc::new(space))),
            Capability::root(),
            Scope::empty(),
        );
        let (screen, cx) =
            cx.add_window_view(|window, cx| Screen::new(worker, "old", Theme::Auto, window, cx));
        start.recv_timeout(Duration::from_secs(2)).unwrap();
        screen.update(cx, |s, cx| s.load("new", cx));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if screen.update(cx, |s, cx| {
                s.poll(cx);
                s.loading.is_none()
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        // Let the older request finish only after the newer face is visible.
        release.wait();
        finish.recv_timeout(Duration::from_secs(2)).unwrap();
        cx.run_until_parked();
        screen.update(cx, |s, cx| {
            s.poll(cx);
            assert!(s.loading.is_none());
            assert_eq!(
                text(s.snapshot.as_ref().unwrap().root(), "content"),
                Some("New")
            );
        });
    }

    #[gpui::test]
    fn contact_inputs_submit_and_keep_values_after_field_refusal(cx: &mut TestAppContext) {
        let store = ikigai_ui_demo::Store::default();
        let root = Arc::new(Fallback::new(vec![
            Arc::new(crate::descriptor_space()),
            ikigai_ui_demo::root(store.clone()),
        ]));
        let worker = Worker::new(
            Arc::new(ikigai_ui_demo::kernel(root)),
            Capability::scoped([ikigai_ui_demo::CONTACT_WRITE]),
            Scope::empty(),
        );
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "contact", Theme::Auto, window, cx));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if screen.update(cx, |s, cx| {
                s.poll(cx);
                s.loading.is_none()
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        // The composed screen can wrap the generated form in groups. Locate controls
        // by their model identity instead of assuming the form is the screen root.
        fn path_to(node: &Node, path: &str, kind: &str, name: Option<&str>) -> Option<String> {
            if node.kind == kind && name.is_none_or(|name| text(node, "name") == Some(name)) {
                return Some(path.to_string());
            }
            node.children
                .iter()
                .enumerate()
                .find_map(|(i, child)| path_to(child, &format!("{path}/{i}"), kind, name))
        }
        let (name_path, email_path, message_path, choice_path, submit_path, form_path) = screen
            .update(cx, |s, _| {
                let root = s.snapshot.as_ref().expect(&s.message).root();
                let path = |kind, name| path_to(root, "/", kind, name).unwrap();
                (
                    path("field", Some("name")),
                    path("field", Some("email")),
                    path("field", Some("message")),
                    path("choice", Some("topic")),
                    path("button", None),
                    path("form", None),
                )
            });
        for (path, value) in [
            (name_path.as_str(), "Ada"),
            (email_path.as_str(), "invalid"),
            (message_path.as_str(), "Hello from GPUI"),
        ] {
            let input = screen.update(cx, |s, _| s.inputs[path].clone());
            cx.update(|window, cx| window.focus(&input.focus_handle(cx)));
            cx.simulate_input(value);
        }
        cx.simulate_keystrokes("enter");
        cx.simulate_input("Second line 🦀");
        let choice_focus = screen.update(cx, |s, _| s.focus[&format!("{choice_path}/1")].clone());
        cx.update(|window, _| window.focus(&choice_focus));
        cx.simulate_keystrokes("space");
        let submit_focus = screen.update(cx, |s, _| s.focus[&submit_path].clone());
        cx.update(|window, _| window.focus(&submit_focus));
        cx.simulate_keystrokes("enter");
        loop {
            if screen.update(cx, |s, cx| {
                s.poll(cx);
                s.forms[&form_path].pending.is_none()
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        cx.update(|_, cx| {
            let screen = screen.read(cx);
            assert!(screen.forms[&form_path]
                .feedback
                .fields
                .contains_key("email"));
            assert_eq!(screen.inputs[&name_path].read(cx).content.as_ref(), "Ada");
            assert_eq!(
                screen.inputs[&message_path].read(cx).content.as_ref(),
                "Hello from GPUI\nSecond line 🦀"
            );
        });
        assert!(store.lock().unwrap().is_empty());
        let input = screen.update(cx, |s, _| s.inputs[&email_path].clone());
        cx.update(|window, cx| window.focus(&input.focus_handle(cx)));
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("ada@example.org");
        cx.update(|window, _| window.focus(&submit_focus));
        cx.simulate_keystrokes("space");
        loop {
            if screen.update(cx, |s, cx| {
                s.poll(cx);
                s.forms[&form_path].pending.is_none()
            }) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(store.lock().unwrap().len(), 1);
        assert_eq!(store.lock().unwrap()[0].topic, "feedback");
        assert_eq!(
            store.lock().unwrap()[0].message,
            "Hello from GPUI\nSecond line 🦀"
        );
        // Tab traversal reaches the first field and then the email field.
        let first = screen.update(cx, |s, _| s.inputs[&name_path].clone());
        cx.update(|window, cx| window.focus(&first.focus_handle(cx)));
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| assert!(input.focus_handle(cx).is_focused(window)));
        cx.update(|_, cx| assert!(screen.read(cx).forms[&form_path].feedback.fields.is_empty()));
    }

    /// Ledger #3516: with the `scene` feature, the gallery's scene is painted off the UI thread
    /// as an image, takes focus, and N / Shift+N / Escape change the announced status. Its
    /// replacement (the graph as text) is installed beside it.
    #[cfg(feature = "scene")]
    #[gpui::test]
    fn a_scene_takes_focus_and_its_selection_is_announced(cx: &mut TestAppContext) {
        let worker = demo_worker(None);
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "gallery", Theme::Light, window, cx));
        settle(&screen, cx);
        let path = screen.update(cx, |s, _| {
            assert_eq!(s.scenes.len(), 1, "{}", s.message);
            s.scenes.keys().next().unwrap().clone()
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let (done, error) = screen.update(cx, |s, cx| {
                s.poll(cx);
                let state = &s.scenes[&path];
                (state.frame.is_some(), state.error.clone())
            });
            if let Some(error) = error {
                eprintln!("SKIPPED painting: {error}");
                return;
            }
            if done {
                break;
            }
            assert!(Instant::now() < deadline, "no frame painted");
            std::thread::sleep(Duration::from_millis(5));
        }
        cx.run_until_parked(); // Draw the image and the replacement beside it.
        let (focus, scene) = screen.update(cx, |s, _| {
            fn at<'a>(node: &'a Node, path: &str) -> &'a Node {
                path.split('/')
                    .filter(|p| !p.is_empty())
                    .fold(node, |n, i| &n.children[i.parse::<usize>().unwrap()])
            }
            let node = at(s.snapshot.as_ref().unwrap().root(), &path);
            assert_eq!(node.kind, "scene");
            assert!(node.replacement.is_some(), "drawn beside the image");
            (
                s.focus[&path].clone(),
                s.scenes[&path].scene.clone().unwrap(),
            )
        });
        cx.update(|window, _| window.focus(&focus));
        cx.simulate_keystrokes("n");
        screen.update(cx, |s, _| {
            assert_eq!(s.scenes[&path].view.selected, Some(0));
            assert_eq!(s.status(), crate::scene::announce(&scene, Some(0)));
            assert!(s.status().starts_with("Selected "), "{}", s.status());
        });
        cx.simulate_keystrokes("n shift-n shift-n");
        screen.update(cx, |s, _| {
            let last = scene.nodes.len() - 1;
            assert_eq!(s.scenes[&path].view.selected, Some(last));
            assert_eq!(s.status(), crate::scene::announce(&scene, Some(last)));
        });
        cx.simulate_keystrokes("left");
        screen.update(cx, |s, _| {
            assert!((s.scenes[&path].view.yaw - 0.5).abs() < 1e-6)
        });
        cx.simulate_keystrokes("escape");
        screen.update(cx, |s, _| {
            assert_eq!(s.scenes[&path].view.selected, None);
            assert_eq!(s.status(), "No node selected.");
        });
        // Each key asked for a new frame, painted off this thread; the last one lands.
        let deadline = Instant::now() + Duration::from_secs(15);
        while screen.update(cx, |s, cx| {
            s.poll(cx);
            s.scenes[&path].painting.is_some()
        }) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The heading a snapshot opens with, to tell screens apart.
    fn first_heading(node: &Node) -> Option<String> {
        if node.kind == "heading" {
            return text(node, "content").map(str::to_string);
        }
        node.children.iter().find_map(first_heading)
    }

    #[gpui::test]
    fn a_page_link_loads_any_screen_the_kernel_names_and_refuses_the_rest(cx: &mut TestAppContext) {
        // A host's own screen, which no list in this crate names.
        let host = EndpointSpace::new().bind(
            Exact::new("urn:iki:ui:screen:name:a-hosts-own:source"),
            FnEndpoint::new("host-screen", |_: &Invocation<'_>| {
                Ok(Representation::new(
                    ReprType::new(ikigai_ui::screen::OUTLINE),
                    b"group label=\"Own\"\n  heading content=\"A host's own screen\" level=1\n"
                        .to_vec(),
                ))
            }),
        );
        let worker = demo_worker(Some(host));
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "gallery", Theme::Light, window, cx));
        settle(&screen, cx);
        let gallery = screen.read_with(cx, |s, _| first_heading(s.snapshot().unwrap().root()));
        // The old fixed list (gallery, elements, contact) never routed it: the route is now the
        // kernel's, not a list.
        screen.update(cx, |s, cx| s.follow("/page/a-hosts-own", cx));
        settle(&screen, cx);
        let elements = screen.read_with(cx, |s, _| {
            assert!(s.message().is_empty(), "{}", s.message());
            first_heading(s.snapshot().unwrap().root())
        });
        assert_ne!(elements, gallery);
        assert_eq!(elements.as_deref(), Some("A host's own screen"));
        // Not a screen name: refused before the worker sees it, and the screen stays.
        for href in ["/page/../secret", "/page/", "/page/a:b", "/elsewhere"] {
            screen.update(cx, |s, cx| s.follow(href, cx));
            settle(&screen, cx);
            screen.read_with(cx, |s, _| {
                assert_eq!(s.message(), format!("No native route for {href}"));
                assert_eq!(first_heading(s.snapshot().unwrap().root()), elements);
            });
        }
        // A screen name the kernel has no screen for: the kernel's refusal, the screen stays.
        screen.update(cx, |s, cx| s.follow("/page/no-such-screen", cx));
        settle(&screen, cx);
        screen.read_with(cx, |s, _| {
            assert!(!s.message().is_empty() && !s.message().starts_with("Loading"));
            assert_eq!(first_heading(s.snapshot().unwrap().root()), elements);
        });
    }

    /// A screen of two choices: `//0` asks for radio buttons, `//1` does not.
    fn radios_screen() -> EndpointSpace {
        let screen = serde_json::json!({"kind": "group", "properties": {"label": "Radios"}, "children": [
            {"kind": "choice", "properties": {"label": "Lens", "name": "lens", "value": "b"},
             "hint": {ikigai_ui::model::RADIO: "radio"}, "children": [
                {"kind": "option", "properties": {"label": "a", "value": "a"}},
                {"kind": "option", "properties": {"label": "b", "value": "b"}},
                {"kind": "option", "properties": {"label": "c", "value": "c"}}]},
            {"kind": "choice", "properties": {"label": "Plain", "name": "plain", "value": "x"}, "children": [
                {"kind": "option", "properties": {"label": "x", "value": "x"}},
                {"kind": "option", "properties": {"label": "y", "value": "y"}}]}]});
        let bytes = serde_json::to_vec(&screen).unwrap();
        EndpointSpace::new().bind(
            Exact::new("urn:iki:ui:screen:name:radios"),
            FnEndpoint::new("test-radios", move |_: &Invocation<'_>| {
                Ok(Representation::new(
                    ReprType::new(ikigai_ui::model::MEDIA_TYPE),
                    bytes.clone(),
                ))
            })
            .with_description(
                Description::new("test-radios")
                    .title("Radios")
                    .summary("A radio choice and a plain one.")
                    .verb(Verb::Source)
                    .output(ikigai_ui::model::MEDIA_TYPE),
            ),
        )
    }

    fn tab_paths(screen: &Entity<Screen>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
        screen.update(cx, |s, _| {
            s.tab_order.iter().map(|(p, _)| p.clone()).collect()
        })
    }

    /// A choice with the radio hint is one radio group (ledger #3537): one tab stop, the arrows
    /// move the selection and wrap, each choice is announced, and Tab leaves the group. A choice
    /// without the hint keeps its selectable options, one stop each.
    #[gpui::test]
    fn a_radio_choice_is_one_focusable_group_whose_arrows_move_the_selection(
        cx: &mut TestAppContext,
    ) {
        let worker = demo_worker(Some(radios_screen()));
        let (screen, cx) =
            cx.add_window_view(|window, cx| Screen::new(worker, "radios", Theme::Auto, window, cx));
        settle(&screen, cx);
        screen.update(cx, |s, _| {
            let root = s.snapshot.as_ref().expect("the radios screen").root();
            assert!(ikigai_ui::model::is_radio(&root.children[0]), "{root:?}");
            assert!(!ikigai_ui::model::is_radio(&root.children[1]));
        });
        assert_eq!(tab_paths(&screen, cx), ["//0", "//1/0", "//1/1"]);
        let group = screen.update(cx, |s, _| s.focus["//0"].clone());
        cx.update(|window, _| window.focus(&group));
        let chosen = |cx: &mut gpui::VisualTestContext| {
            screen.update(cx, |s, _| (s.choices["//0"].clone(), s.message.clone()))
        };
        cx.simulate_keystrokes("down");
        assert_eq!(chosen(cx), ("c".into(), "Lens: c, checked, 3 of 3".into()));
        cx.simulate_keystrokes("down");
        assert_eq!(chosen(cx), ("a".into(), "Lens: a, checked, 1 of 3".into()));
        cx.simulate_keystrokes("up");
        assert_eq!(chosen(cx).0, "c");
        cx.simulate_keystrokes("left");
        assert_eq!(chosen(cx).0, "b");
        cx.simulate_keystrokes("right");
        assert_eq!(chosen(cx).0, "c");
        // The focus stays on the group through the arrows; Tab leaves it for the next control.
        cx.update(|window, _| assert!(group.is_focused(window)));
        cx.simulate_keystrokes("tab");
        let next = screen.update(cx, |s, _| s.focus["//1/0"].clone());
        cx.update(|window, _| assert!(next.is_focused(window)));
        // The plain choice is untouched by the arrows and keeps its value.
        screen.update(cx, |s, _| assert_eq!(s.choices["//1"], "x"));
    }

    /// A GPUI descriptor that does not claim the choice hints drops the hint: the choice keeps
    /// its selectable options, each its own stop.
    #[gpui::test]
    fn without_the_claim_the_radio_choice_keeps_its_options(cx: &mut TestAppContext) {
        let mut descriptor: serde_json::Value = serde_json::from_slice(crate::DESCRIPTOR).unwrap();
        descriptor.as_object_mut().unwrap()["hints"]
            .as_array_mut()
            .unwrap()
            .retain(|ns| ns != ikigai_ui::model::CHOICE_HINTS);
        let bytes = serde_json::to_vec(&descriptor).unwrap();
        let unclaimed = radios_screen().bind(
            Exact::new("urn:iki:ui:target:name:gpui"),
            FnEndpoint::new("test-gpui-no-choice-hints", move |_: &Invocation<'_>| {
                Ok(
                    Representation::new(ReprType::new("application/json"), bytes.clone())
                        .cacheable(),
                )
            })
            .with_description(
                Description::new("test-gpui-no-choice-hints")
                    .title("GPUI without the choice hints")
                    .summary("The native descriptor, claiming no choice hint.")
                    .verb(Verb::Source)
                    .verb(Verb::Meta)
                    .output("application/json"),
            ),
        );
        let worker = demo_worker(Some(unclaimed));
        let (screen, cx) =
            cx.add_window_view(|window, cx| Screen::new(worker, "radios", Theme::Auto, window, cx));
        settle(&screen, cx);
        screen.update(cx, |s, _| {
            let root = s.snapshot.as_ref().unwrap().root();
            assert!(!ikigai_ui::model::is_radio(&root.children[0]), "{root:?}");
        });
        assert_eq!(
            tab_paths(&screen, cx),
            ["//0/0", "//0/1", "//0/2", "//1/0", "//1/1"]
        );
    }

    /// The lensed view's chooser reaches the native face as a radio group. The worker opens only
    /// plain screen names, so the lensed view's GPUI face is read here as the worker reads one,
    /// validated as a snapshot and installed in a window.
    #[gpui::test]
    fn a_lensed_views_chooser_is_a_native_radio_group(cx: &mut TestAppContext) {
        let kernel = ikigai_ui_demo::kernel(Arc::new(Fallback::new(vec![
            Arc::new(crate::descriptor_space()) as Arc<dyn ikigai_core::Space>,
            ikigai_ui_demo::root(Default::default()),
        ])));
        let iri = ikigai_core::Iri::parse(format!(
            "urn:iki:ui:face:target:gpui:screen:lens:contact-list:resource:{}",
            ikigai_ui::scene::EXAMPLE_GRAPH
        ))
        .unwrap();
        let rep = futures::executor::block_on(kernel.issue(
            ikigai_core::Request::new(Verb::Source, iri),
            &Capability::root(),
        ))
        .unwrap();
        let snapshot = Snapshot::from_face(&rep.bytes).unwrap();
        fn find<'a>(n: &'a Node, path: String, out: &mut Vec<(String, &'a Node)>) {
            if n.kind == "choice" {
                out.push((path.clone(), n));
            }
            for (i, c) in n.children.iter().enumerate() {
                find(c, format!("{path}/{i}"), out);
            }
        }
        let mut choices = Vec::new();
        find(snapshot.root(), "/".into(), &mut choices);
        let (path, lens) = choices
            .into_iter()
            .find(|(_, c)| text(c, "name") == Some("lens"))
            .expect("the lens chooser");
        assert!(ikigai_ui::model::is_radio(lens), "{lens:?}");
        let lens = lens.clone();
        let worker = demo_worker(None);
        let (screen, cx) = cx
            .add_window_view(|window, cx| Screen::new(worker, "gallery", Theme::Auto, window, cx));
        settle(&screen, cx);
        screen.update(cx, |s, cx| {
            s.tab_order.clear();
            s.focus.clear();
            s.choices.clear();
            s.install(snapshot.root(), "/", cx);
            s.snapshot = Some(snapshot.clone());
            assert!(s.tab_order.iter().any(|(p, _)| *p == path));
            assert!(!s
                .tab_order
                .iter()
                .any(|(p, _)| p.starts_with(&format!("{path}/"))));
            assert_eq!(s.choices[&path], "contact-list");
        });
        cx.run_until_parked();
        let group = screen.update(cx, |s, _| s.focus[&path].clone());
        cx.update(|window, _| window.focus(&group));
        cx.simulate_keystrokes("down");
        screen.update(cx, |s, _| {
            assert_eq!(s.choices[&path], "graph");
            assert_eq!(s.message, radio_announcement(&lens, 3));
            assert_eq!(s.message, "Lens: graph, checked, 4 of 4");
        });
    }
}
