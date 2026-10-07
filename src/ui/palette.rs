//! The command palette (Ctrl+K), in glass: a dimmed backdrop, a floating
//! panel with a borderless search row, grouped results with the highlighted
//! row as a soft pill, and shortcuts as small keycaps. Up/Down move, Enter
//! runs, Esc (or a click outside) closes; typing filters every group.

use ely_gpui_component::forms::{InputEvent, TextInput};
use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use gpui::{
    AnimationExt, AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, IntoElement,
    KeyDownEvent, MouseButton, Render, SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::ui::glass;

#[derive(Clone)]
pub struct Command {
    /// What the app runs, e.g. "action:new" or "path:<id>".
    pub value: SharedString,
    pub label: SharedString,
    pub icon: IconName,
    /// Shortcut keycaps, e.g. ["Ctrl", "N"].
    pub keys: Vec<&'static str>,
    /// Quiet context after the label: the path a step is in, a path's progress.
    pub detail: Option<SharedString>,
}

impl Command {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>, icon: IconName) -> Self {
        Self { value: value.into(), label: label.into(), icon, keys: Vec::new(), detail: None }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn keys(mut self, keys: &[&'static str]) -> Self {
        self.keys = keys.to_vec();
        self
    }
}

pub struct Group {
    pub title: SharedString,
    pub commands: Vec<Command>,
}

pub enum PaletteEvent {
    Run(SharedString),
    Dismiss,
}

pub struct PaletteView {
    /// Shown before anything is typed.
    suggested: Vec<Group>,
    /// Searched once something is typed: every action, path and step.
    search: Vec<Group>,
    query: Entity<TextInput>,
    /// Index into the filtered, flattened list.
    selected: usize,
    _subscription: Subscription,
}

impl EventEmitter<PaletteEvent> for PaletteView {}

impl Focusable for PaletteView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query.read(cx).focus_handle(cx)
    }
}

/// Whether `label` matches every word typed, in any order.
fn matches(label: &str, query: &str) -> bool {
    let label = label.to_lowercase();
    query.split_whitespace().all(|word| label.contains(&word.to_lowercase()))
}

impl PaletteView {
    pub fn new(suggested: Vec<Group>, search: Vec<Group>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| TextInput::new(window, cx).placeholder("Search actions, paths and every step"));
        let _subscription = cx.subscribe(&query, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Changed => {
                this.selected = 0;
                cx.notify();
            }
            InputEvent::Submit => this.run_selected(cx),
            _ => {}
        });
        query.read(cx).focus_handle(cx).focus(window, cx);
        Self { suggested, search, query, selected: 0, _subscription }
    }

    /// Suggestions while the query is empty; otherwise every group with only
    /// its matching commands (label or detail), empty groups dropped.
    fn filtered(&self, cx: &App) -> Vec<(SharedString, Vec<Command>)> {
        const PER_GROUP: usize = 40;
        let query = self.query.read(cx).text().trim().to_string();
        if query.is_empty() {
            return self.suggested.iter().map(|g| (g.title.clone(), g.commands.clone())).collect();
        }
        self.search
            .iter()
            .map(|group| {
                let commands: Vec<Command> = group
                    .commands
                    .iter()
                    .filter(|c| {
                        let haystack = match &c.detail {
                            Some(detail) => format!("{} {detail}", c.label),
                            None => c.label.to_string(),
                        };
                        matches(&haystack, &query)
                    })
                    .take(PER_GROUP)
                    .cloned()
                    .collect();
                (group.title.clone(), commands)
            })
            .filter(|(_, commands)| !commands.is_empty())
            .collect()
    }

    fn run_selected(&mut self, cx: &mut Context<Self>) {
        let chosen = self.filtered(cx).into_iter().flat_map(|(_, commands)| commands).nth(self.selected);
        if let Some(command) = chosen {
            cx.emit(PaletteEvent::Run(command.value));
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let count = self.filtered(cx).iter().map(|(_, c)| c.len()).sum::<usize>();
        match event.keystroke.key.as_str() {
            "down" if count > 0 => self.selected = (self.selected + 1) % count,
            "up" if count > 0 => self.selected = (self.selected + count - 1) % count,
            "escape" => cx.emit(PaletteEvent::Dismiss),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn keycap(key: &'static str, cx: &App) -> AnyElement {
        div()
            .flex_none()
            .px(px(6.))
            .h(px(20.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(glass::ink(cx, 0.05))
            .border_1()
            .border_color(glass::ink(cx, 0.12))
            .text_size(px(11.))
            .font_family(cx.theme().mono_family.clone())
            .text_color(cx.theme().colors.fg_muted)
            .child(key)
            .into_any_element()
    }
}

impl Render for PaletteView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.filtered(cx);
        let c = cx.theme().colors.clone();
        let light = glass::is_light(cx);
        let total: usize = groups.iter().map(|(_, c)| c.len()).sum();
        self.selected = self.selected.min(total.saturating_sub(1));

        let mut index = 0;
        let mut sections: Vec<AnyElement> = Vec::new();
        for (title, commands) in groups {
            let mut rows: Vec<AnyElement> = Vec::new();
            for command in commands {
                let ix = index;
                index += 1;
                let selected = ix == self.selected;
                let (hover, value) = (glass::ink(cx, 0.05), command.value.clone());
                rows.push(
                    div()
                        .id(("palette-row", ix))
                        .flex()
                        .items_center()
                        .gap_3()
                        .h(px(40.))
                        .px_3()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(if selected { glass::ink(cx, 0.1) } else { gpui::transparent_black() })
                        .when(selected, |row| row.bg(glass::ink(cx, 0.07)))
                        .when(!selected, |row| row.hover(move |s| s.bg(hover)))
                        .cursor_pointer()
                        .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                            if *hovering && this.selected != ix {
                                this.selected = ix;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.emit(PaletteEvent::Run(value.clone()))
                        }))
                        .child(
                            div()
                                .flex_none()
                                .size(px(6.))
                                .rounded_full()
                                .bg(if selected { c.accent } else { gpui::transparent_black() }),
                        )
                        .child(Icon::new(command.icon).size(IconSize::Sm).color(if selected {
                            c.fg
                        } else {
                            c.fg_muted
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_baseline()
                                .gap_2()
                                .child(div().flex_none().max_w(px(340.)).text_ellipsis().child(command.label.clone()))
                                .children(command.detail.clone().map(|detail| {
                                    div()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .text_color(c.fg_subtle)
                                        .child(detail)
                                })),
                        )
                        .children(command.keys.iter().map(|key| Self::keycap(key, cx)))
                        .into_any_element(),
                );
            }
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(glass::eyebrow(title, cx).px_3().pt_2().pb_1())
                    .children(rows)
                    .into_any_element(),
            );
        }
        let results_height = if total == 0 {
            120.
        } else {
            (total as f32 * 42. + sections.len() as f32 * 34. + 16.).min(380.)
        };
        let empty = (total == 0).then(|| {
            div()
                .py_10()
                .flex()
                .justify_center()
                .text_color(c.fg_muted)
                .child("Nothing matches. Try fewer words.")
        });

        let panel = div()
            .id("palette-panel")
            .w(px(640.))
            .flex()
            .flex_col()
            .rounded(px(22.))
            .bg(glass::solid_fill(cx))
            .border_1()
            .border_color(glass::ink(cx, 0.12))
            .shadow(vec![gpui::BoxShadow {
                color: glass::shade(cx, 0.6),
                offset: gpui::point(px(0.), px(24.)),
                blur_radius: px(60.),
                spread_radius: px(0.),
                inset: false,
            }])
            .overflow_hidden()
            // Clicks inside must not reach the backdrop, which closes.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(56.))
                    .px_5()
                    .border_b_1()
                    .border_color(glass::ink(cx, 0.08))
                    .child(Icon::new(IconName::Search).size(IconSize::Md).color(c.fg_muted))
                    .child(div().flex_1().text_size(px(16.)).child(self.query.clone()))
                    .child(Self::keycap("Esc", cx)),
            )
            .child(
                // Sized to the rows (40px each, plus group labels), up to a cap;
                // a scroll area needs a definite height.
                ScrollArea::new("palette-results")
                    .h(px(results_height))
                    .child(div().flex().flex_col().gap_1().p_2().children(sections).children(empty)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.))
                    .px_5()
                    .border_t_1()
                    .border_color(glass::ink(cx, 0.08))
                    .text_size(px(12.))
                    .text_color(c.fg_subtle)
                    .child(Self::keycap("\u{2191}", cx))
                    .child(Self::keycap("\u{2193}", cx))
                    .child("to move")
                    .child(div().w_2())
                    .child(Self::keycap("Enter", cx))
                    .child("to run")
                    .child(div().flex_1())
                    .child(div().font_weight(FontWeight::MEDIUM).child(format!(
                        "{total} result{}",
                        if total == 1 { "" } else { "s" }
                    ))),
            )
            .with_animation(
                "palette-in",
                gpui::Animation::new(std::time::Duration::from_millis(180)).with_easing(gpui::ease_out_quint()),
                |panel, d| panel.opacity(d).mt(px(96. + 10. * (1. - d))),
            );

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .bg(gpui::black().opacity(if light { 0.18 } else { 0.5 }))
            .capture_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .child(panel)
    }
}
