//! GPUI elements (macOS windows; the same walker also compiles on Linux). Rendering never invokes a kernel or waits for a composition.
use std::{collections::BTreeMap, time::Duration};

use gpui::{div, prelude::*, px, rgb, App, Context, Entity, Focusable, Window};
use ikigai_ui::model::{Node, Value};

use crate::{
    text,
    worker::{Feedback, Pending, Worker},
    Snapshot,
};
#[path = "input.rs"]
mod input;
use input::TextInput;

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
    tab_order: Vec<gpui::FocusHandle>,
}

impl Screen {
    /// Start loading without blocking the window. Polling is driven by a foreground
    /// timer; all composition and Sink calls run on the worker's OS threads.
    pub fn new(worker: Worker, screen: &str, cx: &mut Context<Self>) -> Self {
        input::bind_keys(cx);
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
        }
    }

    fn load(&mut self, name: &str, cx: &mut Context<Self>) {
        self.loading = Some(self.worker.load(name));
        self.message = "Loading…".into();
        cx.notify();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        if let Some(result) = self.loading.as_ref().and_then(Pending::poll) {
            self.loading = None;
            match result {
                Ok(snapshot) => {
                    self.inputs.clear();
                    self.choices.clear();
                    self.forms.clear();
                    self.focus.clear();
                    self.tab_order.clear();
                    self.install(snapshot.root(), "/", cx);
                    self.snapshot = Some(snapshot);
                    self.message.clear();
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
        if changed {
            cx.notify();
        }
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
                let input = cx.new(|cx| {
                    let mut input = TextInput::new(value, cx);
                    input.multiline = text(node, "input") == Some("textarea");
                    input
                });
                let focus = input.focus_handle(cx);
                self.tab_order.push(focus.clone());
                self.focus.insert(path.into(), focus);
                self.inputs.insert(path.into(), input);
            }
            "choice" => {
                self.choices.insert(
                    path.into(),
                    text(node, "value").unwrap_or_default().to_string(),
                );
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
            _ => {}
        }
        if ["button", "link", "option"].contains(&node.kind.as_str()) {
            let focus = cx.focus_handle();
            self.tab_order.push(focus.clone());
            self.focus.insert(path.into(), focus);
        }
        for (index, child) in node.children.iter().enumerate() {
            self.install(child, &format!("{path}/{index}"), cx);
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

    fn follow(&mut self, href: &str, cx: &mut Context<Self>) {
        if let Some(name) = href
            .strip_prefix("/page/")
            .filter(|n| ["gallery", "contact"].contains(n))
        {
            self.load(name, cx);
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
                        .focus(|s| s.border_1().border_color(rgb(0x2563eb)))
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
            "choice" => {
                body = body.child(label());
                for (i, option) in node.children.iter().enumerate() {
                    let value = text(option, "value").unwrap_or_default().to_string();
                    let selected = self.choices[path] == value;
                    let focus = self.focus[&format!("{path}/{i}")].clone();
                    let path = path.to_string();
                    let key_path = path.clone();
                    let key_value = value.clone();
                    body = body.child(
                        div()
                            .id(i)
                            .p_2()
                            .border_1()
                            .cursor_pointer()
                            .track_focus(&focus)
                            .focus(|s| s.border_color(rgb(0x2563eb)))
                            .bg(rgb(if selected { 0xdbeafe } else { 0xffffff }))
                            .child(format!(
                                "{} {}",
                                if selected { "●" } else { "○" },
                                text(option, "label").unwrap_or_default()
                            ))
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
                let mut button = div()
                    .id(gpui::SharedString::from(path.to_string()))
                    .track_focus(&focus)
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0x1e40af))
                    .text_color(rgb(0xffffff))
                    .child(label())
                    .focus(|s| s.border_2().border_color(rgb(0x60a5fa)));
                if let Some((form, form_path)) = form {
                    let busy = self.forms[form_path].pending.is_some();
                    if !busy {
                        let (form, form_path, node) =
                            (form.clone(), form_path.to_string(), node.clone());
                        let (key_form, key_path, key_node) =
                            (form.clone(), form_path.clone(), node.clone());
                        button = button
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x1d4ed8)))
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
                    } else {
                        button = button.child(" (Submitting…)");
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
                    .text_color(rgb(0x1d4ed8))
                    .cursor_pointer()
                    .focus(|s| s.underline())
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
            "form" | "group" | "list" | "nav" | "table" => {
                body = body.child(if node.kind == "table" {
                    text(node, "caption").unwrap_or_default().to_string()
                } else {
                    label()
                });
                if node.kind == "form" {
                    body = body
                        .p_4()
                        .border_1()
                        .border_color(rgb(0x94a3b8))
                        .rounded_md();
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
                        let mut row = div().flex().flex_row().gap_4().border_b_1();
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
            body = body.child(div().text_color(rgb(0xb91c1c)).child(error.to_string()));
        }
        body.into_any_element()
    }
}

impl Render for Screen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("screen")
            .size_full()
            .overflow_y_scroll()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" && !this.tab_order.is_empty() {
                    let current = this
                        .tab_order
                        .iter()
                        .position(|focus| focus.is_focused(window));
                    let len = this.tab_order.len();
                    let next = match (current, event.keystroke.modifiers.shift) {
                        (Some(i), true) => (i + len - 1) % len,
                        (Some(i), false) => (i + 1) % len,
                        (None, true) => len - 1,
                        (None, false) => 0,
                    };
                    window.focus(&this.tab_order[next]);
                    cx.stop_propagation();
                }
            }))
            .bg(rgb(0xf8fafc))
            .text_color(rgb(0x0f172a))
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
        let (screen, cx) = cx.add_window_view(|_, cx| Screen::new(worker, "gallery", cx));
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
        let (screen, cx) = cx.add_window_view(|_, cx| Screen::new(worker, "slow", cx));
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
        let (screen, cx) = cx.add_window_view(|_, cx| Screen::new(worker, "old", cx));
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
        let (screen, cx) = cx.add_window_view(|_, cx| Screen::new(worker, "contact", cx));
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
}
