//! The path header (editable name, progress, actions) and the floating map
//! controls (zoom and a shortcuts reference).

use ely_gpui_component::menus::{Menu, MenuItem, OverflowMenu};
use ely_gpui_component::canvas::Viewport;
use ely_gpui_component::forms::InlineEdit;
use ely_gpui_component::motion::Spinner;
use ely_gpui_component::primitives::{Icon, IconName, TooltipTrigger};
use ely_gpui_component::theme::{ActiveTheme, Elevation, IconSize, TextSize};
use ely_gpui_component::typography::{Caption, Kbd, Label};
use gpui::{
    ClickEvent, Context, FontWeight, IntoElement, MouseButton, div, prelude::*, px,
};

use super::{MapEvent, MindMapView, ViewMode, ZOOM_STEP};
use crate::ui::glass::{self, PillStyle};

const SHORTCUTS: &[(&str, &str)] = &[
    ("up down left right", "Move between cards"),
    ("space", "Mark the subtopic done"),
    ("escape", "Close the inspector"),
    ("ctrl-0", "Fit the whole path"),
    ("ctrl-= ctrl--", "Zoom in and out"),
    ("ctrl-k", "Command palette"),
];

impl MindMapView {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let c = theme.colors.clone();
        let mono = theme.mono_family.clone();
        let (done, total) = self.path.progress();
        let view = cx.entity().downgrade();
        let view_for_mode = view.clone();
        let menu = {
            let (export, reset) = (view.clone(), view.clone());
            Menu::new()
                .item(
                    MenuItem::new("Export as Markdown")
                        .icon(IconName::Download)
                        .on_click(move |_, cx| {
                            export.update(cx, |this, cx| this.export_markdown(cx)).ok();
                        }),
                )
                .item(
                    MenuItem::new("Reset card layout")
                        .icon(IconName::RotateCcw)
                        .disabled(self.path.positions.is_empty())
                        .on_click(move |_, cx| {
                            reset.update(cx, |this, cx| this.reset_layout(cx)).ok();
                        }),
                )
        };
        let focused = self
            .focus_topic
            .and_then(|t| self.path.topics.get(t))
            .map(|topic| topic.name.clone());

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap_3()
            .h(theme.titlebar_height() * 1.25)
            .pl_3()
            .pr_4()
            .border_b_1()
            .border_color(c.border)
            .child(
                div()
                    .min_w_0()
                    .max_w_96()
                    .text_size(theme.text_size(TextSize::Xl))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        InlineEdit::new("path-name", self.path.name.clone())
                            .placeholder("Untitled path")
                            .on_commit(move |name, _, cx| {
                                view.update(cx, |this, cx| this.rename(name.to_string(), cx)).ok();
                            }),
                    ),
            )
            .child(div().flex_none().child(Caption::new(self.path.summary())))
            .when_some(focused, |row, name| {
                row.child(
                    glass::pill("clear-focus", format!("Focused on {name}"), Some(IconName::X), PillStyle::Selected, cx)
                        .h(px(28.))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            if let Some(topic) = this.focus_topic {
                                this.toggle_focus(topic, cx);
                            }
                        })),
                )
            })
            .child(div().flex_1())
            // The controls keep their size; the path name gives way first.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(glass::segmented(
                        "view-mode",
                        vec![
                            glass::Segment::new("map", "Map").icon(IconName::Network),
                            glass::Segment::new("outline", "Outline").icon(IconName::ListChecks),
                        ],
                        if self.view_mode == ViewMode::Map { "map" } else { "outline" },
                        {
                            let view = view_for_mode.clone();
                            move |value, _, cx| {
                                let mode = if value.as_ref() == "outline" { ViewMode::Outline } else { ViewMode::Map };
                                view.update(cx, |this, cx| {
                                    this.view_mode = mode;
                                    cx.notify();
                                })
                                .ok();
                            }
                        },
                        cx,
                    ))
            .when_some(self.path.building.clone(), |row, status| {
                row.child(Spinner::new("path-building"))
                    .child(Caption::new(format!("{status}\u{2026}")))
                    .child(
                        glass::pill("stop-build", "Stop", Some(IconName::Square), PillStyle::Ghost, cx)
                            .h(px(28.))
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(MapEvent::StopBuild))),
                    )
            })
            .when(self.path.building.is_none(), |row| {
                row.child(div().w_32().child(glass::meter(done as f32 / total.max(1) as f32, cx)))
                    .child(Caption::new(format!("{done} of {total} done")).font_family(mono.clone()))
            })
            .child(div().w_px().h_4().bg(c.border))
            .child(Icon::new(IconName::Clock).size(IconSize::Sm).color(c.fg_subtle))
            .child(Caption::new(self.path.estimated_time.clone()).font_family(mono))
            .child(OverflowMenu::new("path-menu", menu).tooltip("More")),
            )
    }

    pub(super) fn render_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let current = self.viewport.zoom;
        let zoom = div()
            .flex()
            .items_center()
            .gap_1()
            .child(glass::icon_button("zoom-out", IconName::ZoomOut, "Zoom out (Ctrl -)", cx).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| this.zoom_to((current / ZOOM_STEP).max(Viewport::ZOOMS.0), cx),
            )))
            .child(
                div()
                    .w(px(44.))
                    .flex()
                    .justify_center()
                    .text_size(px(12.))
                    .font_family(cx.theme().mono_family.clone())
                    .text_color(cx.theme().colors.fg)
                    .child(format!("{:.0}%", current * 100.)),
            )
            .child(glass::icon_button("zoom-in", IconName::ZoomIn, "Zoom in (Ctrl =)", cx).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| this.zoom_to((current * ZOOM_STEP).min(Viewport::ZOOMS.1), cx),
            )))
            .child(
                glass::icon_button("zoom-fit", IconName::Maximize2, "Fit the whole path (Ctrl 0)", cx)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.fit_view(cx))),
            );
        let fill = glass::panel(cx);
        let theme = cx.theme();
        let c = theme.colors.clone();
        let help = TooltipTrigger::new(
            "map-help",
            div()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(c.border)
                .hover(|s| s.bg(c.hover))
                .child(Icon::new(IconName::CircleHelp).size(IconSize::Sm).color(c.fg_muted)),
            |_, _| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_1()
                    .child(Label::new("Map shortcuts"))
                    .children(SHORTCUTS.iter().map(|(keys, what)| {
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_6()
                            .child(Caption::new(*what))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .children(keys.split(' ').map(Kbd::new)),
                            )
                    }))
                    .child(Caption::new("Drag empty space to pan; Ctrl + scroll to zoom."))
                    .into_any_element()
            },
        );
        div()
            .absolute()
            .left_4()
            .bottom_4()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .px_2()
            .rounded_full()
            .bg(fill)
            .border_1()
            .border_color(c.border)
            .shadow(theme.elevation(Elevation::Floating))
            // Presses here must not reach the plane and clear the selection.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(zoom)
            .child(div().w_px().h_4().mx_1().bg(c.border))
            .child(help)
    }
}
