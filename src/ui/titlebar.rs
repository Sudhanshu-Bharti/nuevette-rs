//! The window title bar: transparent over the glow, wordmark left, nav and
//! caption buttons right. On Windows, Ely's `TitleBar` arms a window drag on
//! any press inside the bar, caption buttons included, so the slightest
//! movement during a click on minimize or close starts a move loop that eats
//! the release. Here, as in Zed's own title bar, the caption buttons occlude
//! the drag area and leave the click to Windows: their `WindowControlArea`
//! hit-tests as the native minimize, maximize and close buttons.

use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::shell::TitleBar;
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use gpui::{AnyElement, App, IntoElement, SharedString, Window, WindowControlArea, div, prelude::*, px};

pub fn title_bar(leading: impl IntoElement, trailing: impl IntoElement, window: &mut Window, cx: &mut App) -> AnyElement {
    if !cfg!(target_os = "windows") {
        return TitleBar::new("titlebar").child(leading).action(trailing).into_any_element();
    }
    div()
        .id("titlebar")
        // Windows drags the window from here and maximizes on double-click.
        .window_control_area(WindowControlArea::Drag)
        .flex()
        .flex_none()
        .items_center()
        .gap_3()
        .h(px(56.))
        .pl_5()
        .child(leading)
        .child(div().flex_1())
        // A press here is a click, not the start of a drag.
        .child(div().flex_none().occlude().child(trailing))
        .child(div().flex_none().self_start().child(caption_buttons(window, cx)))
        .into_any_element()
}

fn caption_buttons(window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let c = &theme.colors;
    let maximize = if window.is_maximized() { IconName::Copy } else { IconName::Square };
    let buttons = [
        ("minimize", IconName::Minus, WindowControlArea::Min),
        ("maximize", maximize, WindowControlArea::Max),
        ("close", IconName::X, WindowControlArea::Close),
    ];
    let height = theme.caption_button_height(window.rem_size());
    div().flex().flex_none().children(buttons.map(|(name, icon, area)| {
        let group = SharedString::from(format!("caption-{name}"));
        let (hover, glyph) = match area {
            WindowControlArea::Close => (c.danger, c.on_accent),
            _ => (c.hover, c.fg),
        };
        div()
            .id(group.clone())
            .group(group.clone())
            .flex()
            .items_center()
            .justify_center()
            .w(theme.caption_button_width())
            .h(height)
            .occlude()
            .window_control_area(area)
            .hover(|style| style.bg(hover))
            .child(
                Icon::new(icon)
                    .size(IconSize::Sm)
                    .color(c.fg_muted)
                    .group_hover_color(group, glyph),
            )
    }))
}
