//! The glass kit: translucent cards with a lit top edge over the teal glow,
//! pills, chips, uppercase eyebrows and big stat numbers. GPUI has no
//! backdrop blur, so "glass" is a faint fill, hairline borders and a soft
//! shadow; the glow image underneath does the rest.

use ely_gpui_component::primitives::{Icon, IconName, Tooltip};
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use gpui::{
    AnyElement, App, BoxShadow, Div, ElementId, FontWeight, Hsla, IntoElement, ObjectFit, ParentElement, RenderOnce,
    SharedString, Stateful, Window, div, img, linear_color_stop, linear_gradient, point, prelude::*, px,
};

use crate::model::LearningPath;

const CARD_RADIUS: f32 = 18.;
const DENSE_RADIUS: f32 = 14.;

/// Splits a stat into its whole part and the rest, which is drawn dimmer:
/// "6.5 h" -> ("6", ".5 h"), "71.4%" -> ("71", ".4%").
pub fn split_number(text: &str) -> (String, String) {
    let end = text
        .char_indices()
        .find(|(i, ch)| !(ch.is_ascii_digit() || *ch == ',' || (*i == 0 && matches!(ch, '~' | '-' | '+'))))
        .map_or(text.len(), |(i, _)| i);
    (text[..end].to_string(), text[end..].to_string())
}

/// Whether the light ("frosted") theme is showing.
pub fn is_light(cx: &App) -> bool {
    !cx.theme().is_dark()
}

/// A translucent overlay or hairline in the text color: white-ish on dark,
/// ink on light.
pub fn ink(cx: &App, alpha: f32) -> Hsla {
    cx.theme().colors.fg.opacity(alpha)
}

/// Glass card fill: a faint white veil on dark, white frost on light.
pub fn card_fill(cx: &App, dense: bool) -> Hsla {
    match (is_light(cx), dense) {
        (true, false) => white(0.74),
        (true, true) => white(0.62),
        (false, false) => white(0.035),
        (false, true) => white(0.025),
    }
}

/// An opaque glass fill, for cards and panels that float over busy content.
pub fn solid_fill(cx: &App) -> Hsla {
    let c = &cx.theme().colors;
    crate::theme::blend(c.surface, c.fg, if is_light(cx) { 0. } else { 0.035 })
}

/// Drop-shadow color; light mode wants far less of it.
pub fn shade(cx: &App, alpha: f32) -> Hsla {
    gpui::black().opacity(alpha * if is_light(cx) { 0.3 } else { 1. })
}

/// The baked glow (see `scripts/glow.py`), pinned to the top of its parent,
/// in the current theme's tint and at the strength chosen in Settings. A
/// failed load draws nothing, leaving the plain base.
pub fn glow(opacity: f32, cx: &App) -> impl IntoElement {
    let strength = cx.global::<crate::settings::Settings>().glow.opacity();
    img(if is_light(cx) { "glow-light.png" } else { "glow.png" })
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(640.))
        // Stretched, not cropped: Cover would centre the image and push its
        // bright top out of view on wide windows. The glow is soft enough.
        .object_fit(ObjectFit::Fill)
        .opacity(opacity * strength)
}

/// Fill for side panels and pinned bars that sit over busy content.
pub fn panel(cx: &App) -> Hsla {
    cx.theme().colors.surface.opacity(0.94)
}

fn white(alpha: f32) -> Hsla {
    gpui::white().opacity(alpha)
}

/// A 1px highlight along a card's top edge, bright in the middle.
fn sheen() -> Div {
    let (clear, lit) = (white(0.), white(0.16));
    div()
        .absolute()
        .top_0()
        .left(px(CARD_RADIUS))
        .right(px(CARD_RADIUS))
        .h_px()
        .flex()
        .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(clear, 0.), linear_color_stop(lit, 1.))))
        .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(lit, 0.), linear_color_stop(clear, 1.))))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hue {
    Accent,
    Success,
    Neutral,
}

impl Hue {
    fn color(self, cx: &App) -> Hsla {
        let c = &cx.theme().colors;
        match self {
            Hue::Accent => c.accent,
            Hue::Success => c.success,
            Hue::Neutral => c.fg_muted,
        }
    }
}

/// Small uppercase status chip: tinted fill, border and text.
pub fn chip(text: impl Into<SharedString>, hue: Hue, cx: &App) -> Div {
    let tint = hue.color(cx);
    let text: SharedString = text.into();
    div()
        .flex_none()
        .px_2()
        .py(px(2.))
        .rounded_full()
        .bg(tint.opacity(0.12))
        .border_1()
        .border_color(tint.opacity(0.3))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tint)
        .child(text.to_uppercase())
}

/// Small uppercase label over a value or a section.
pub fn eyebrow(text: impl Into<SharedString>, cx: &App) -> Div {
    let text: SharedString = text.into();
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.fg_muted)
        .child(text.to_uppercase())
}

/// A big, light number; whatever follows the whole part is dimmed.
pub fn stat_value(text: &str, cx: &App) -> Div {
    let c = &cx.theme().colors;
    let (whole, rest) = split_number(text);
    div()
        .flex()
        .items_baseline()
        .text_size(px(40.))
        .line_height(px(48.))
        .font_weight(FontWeight::LIGHT)
        .child(div().text_color(c.fg).child(whole))
        .child(div().text_color(c.fg_subtle).child(rest))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillStyle {
    Primary,
    Ghost,
    Selected,
    /// No fill or border until hovered: unselected segments.
    Quiet,
}

/// A fully rounded button; the caller adds `.on_click`.
pub fn pill(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: Option<IconName>,
    style: PillStyle,
    cx: &App,
) -> Stateful<Div> {
    let c = &cx.theme().colors;
    let (bg, fg, border, hover) = match style {
        PillStyle::Primary => (c.accent, c.on_accent, c.accent, c.accent_hover),
        PillStyle::Ghost => (ink(cx, 0.04), c.fg, ink(cx, 0.1), ink(cx, 0.08)),
        PillStyle::Selected => (ink(cx, 0.1), c.fg, ink(cx, 0.16), ink(cx, 0.12)),
        PillStyle::Quiet => (ink(cx, 0.), c.fg_muted, ink(cx, 0.), ink(cx, 0.06)),
    };
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap_1p5()
        .h(px(32.))
        .px_3p5()
        .rounded_full()
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(|s| s.opacity(0.85))
        .children(icon.map(|icon| Icon::new(icon).size(IconSize::Sm).color(fg)))
        .child(label.into())
}

/// Dims a pill or round icon and drops its pointer; the caller leaves out
/// `.on_click` while it is disabled.
pub fn disabled(button: Stateful<Div>) -> Stateful<Div> {
    button.opacity(0.35).cursor_default()
}

/// A muted key hint inside a pill, e.g. "Space" on "Mark done & next".
pub fn key_hint(keys: &'static str, color: Hsla) -> Div {
    div().pl_1().text_size(px(12.)).text_color(color.opacity(0.65)).child(keys)
}

/// One option of a segmented control.
pub struct Segment {
    pub value: SharedString,
    pub label: SharedString,
    pub icon: Option<IconName>,
}

impl Segment {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self { value: value.into(), label: label.into(), icon: None }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// The selected value is raised; every other segment stays quiet.
fn segment_styles(values: &[&str], selected: &str) -> Vec<PillStyle> {
    values
        .iter()
        .map(|value| if *value == selected { PillStyle::Selected } else { PillStyle::Quiet })
        .collect()
}

/// A glass capsule of pills, one raised, like the nav.
pub fn segmented(
    id: impl Into<SharedString>,
    segments: Vec<Segment>,
    selected: &str,
    on_change: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let id: SharedString = id.into();
    let on_change = std::rc::Rc::new(on_change);
    let values: Vec<&str> = segments.iter().map(|s| s.value.as_ref()).collect();
    let styles = segment_styles(&values, selected);
    let c = &cx.theme().colors;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_0p5()
        .p(px(3.))
        .rounded_full()
        .bg(ink(cx, 0.03))
        .border_1()
        .border_color(c.border)
        .children(segments.into_iter().zip(styles).enumerate().map(|(ix, (segment, style))| {
            let handler = on_change.clone();
            let value = segment.value.clone();
            pill((id.clone(), ix), segment.label, segment.icon, style, cx)
                .h(px(28.))
                .px_3()
                .on_click(move |_, window, cx| handler(&value, window, cx))
        }))
        .into_any_element()
}

/// 0..=1, with NaN as 0, so a bar never overflows its track.
fn meter_share(share: f32) -> f32 {
    if share.is_nan() { 0. } else { share.clamp(0., 1.) }
}

/// A thin rounded progress track; teal while going, green when complete.
pub fn meter(share: f32, cx: &App) -> Div {
    let c = &cx.theme().colors;
    let share = meter_share(share);
    div()
        .h(px(4.))
        .w_full()
        .rounded_full()
        .bg(ink(cx, 0.08))
        .child(
            div()
                .h_full()
                .w(gpui::relative(share))
                .rounded_full()
                .bg(if share >= 1. { c.success } else { c.accent }),
        )
}

/// A round icon button for the nav; the caller adds `.on_click` and a tooltip.
pub fn round_icon(id: impl Into<ElementId>, icon: IconName, cx: &App) -> Stateful<Div> {
    let c = &cx.theme().colors;
    let hover = ink(cx, 0.08);
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(32.))
        .rounded_full()
        .border_1()
        .border_color(ink(cx, 0.08))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(|s| s.opacity(0.85))
        .child(Icon::new(icon).size(IconSize::Sm).color(c.fg_muted))
}

/// A small round icon button with a tooltip, for rows and panels.
pub fn icon_button(id: impl Into<ElementId>, icon: IconName, tip: &'static str, cx: &App) -> Stateful<Div> {
    round_icon(id, icon, cx).size(px(28.)).tooltip(Tooltip::text(tip))
}

/// A round glass badge holding an icon, for the corner of a stat card.
pub fn badge(icon: IconName, cx: &App) -> Div {
    let c = &cx.theme().colors;
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(36.))
        .rounded_full()
        .bg(ink(cx, 0.05))
        .border_1()
        .border_color(ink(cx, 0.12))
        .child(Icon::new(icon).size(IconSize::Sm).color(c.fg))
}

/// A path's color circle with its initial.
pub fn avatar(path: &LearningPath, cx: &App) -> Div {
    let hue = cx.theme().colors.hue(path.hue(), "path color");
    let initial: String = path
        .name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(36.))
        .rounded_full()
        .bg(hue.opacity(0.16))
        .border_1()
        .border_color(hue.opacity(0.4))
        .text_size(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(hue)
        .child(initial)
}

/// A glass card: optional title row, body, optional footer strip.
#[derive(IntoElement)]
pub struct GlassCard {
    dense: bool,
    title: Option<SharedString>,
    description: Option<SharedString>,
    action: Option<AnyElement>,
    body: Vec<AnyElement>,
    footer: Option<AnyElement>,
}

impl GlassCard {
    pub fn new() -> Self {
        Self { dense: false, title: None, description: None, action: None, body: Vec::new(), footer: None }
    }

    /// Smaller radius and quieter shadow, for rows and lists.
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }
}

impl Default for GlassCard {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for GlassCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements);
    }
}

impl RenderOnce for GlassCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = cx.theme().colors.clone();
        let radius = if self.dense { DENSE_RADIUS } else { CARD_RADIUS };
        let pad = px(if self.dense { 16. } else { 20. });
        let shadow = BoxShadow {
            color: shade(cx, if self.dense { 0.22 } else { 0.35 }),
            offset: point(px(0.), px(if self.dense { 4. } else { 8. })),
            blur_radius: px(if self.dense { 12. } else { 24. }),
            spread_radius: px(0.),
            inset: false,
        };
        let header = (self.title.is_some() || self.action.is_some()).then(|| {
            div()
                .flex()
                .items_start()
                .gap_3()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .children(self.title.map(|t| div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(t)))
                        .children(self.description.map(|d| div().text_size(px(13.)).text_color(c.fg_muted).child(d))),
                )
                .children(self.action)
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .rounded(px(radius))
            .bg(card_fill(cx, self.dense))
            .border_1()
            .border_color(c.border)
            .shadow(vec![shadow])
            .child(sheen())
            .child(div().flex().flex_col().gap_4().p(pad).children(header).children(self.body))
            .children(self.footer.map(|footer| {
                div()
                    .flex()
                    .items_center()
                    .px(pad)
                    .py_3()
                    .border_t_1()
                    .border_color(ink(cx, 0.06))
                    .child(footer)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_selected_segment_is_raised() {
        let styles = segment_styles(&["map", "outline"], "outline");
        assert_eq!(styles, vec![PillStyle::Quiet, PillStyle::Selected]);
        assert_eq!(segment_styles(&["a", "b"], "missing"), vec![PillStyle::Quiet, PillStyle::Quiet]);
    }

    #[test]
    fn meter_share_is_clamped() {
        assert_eq!(meter_share(0.4), 0.4);
        assert_eq!(meter_share(-1.), 0.);
        assert_eq!(meter_share(3.), 1.);
        assert_eq!(meter_share(f32::NAN), 0.);
    }

    #[test]
    fn split_number_dims_what_follows_the_whole_part() {
        assert_eq!(split_number("6.5 h"), ("6".into(), ".5 h".into()));
        assert_eq!(split_number("71.4%"), ("71".into(), ".4%".into()));
        assert_eq!(split_number("12"), ("12".into(), String::new()));
        assert_eq!(split_number("4 days"), ("4".into(), " days".into()));
        assert_eq!(split_number("~33 hours"), ("~33".into(), " hours".into()));
        assert_eq!(split_number("-3"), ("-3".into(), String::new()));
        assert_eq!(split_number(""), (String::new(), String::new()));
    }
}
