//! The wordmark and the pill nav in the title bar. The active screen shows
//! as a labelled pill; the rest are round icon buttons with tooltips.

use std::rc::Rc;

use ely_gpui_component::primitives::{Icon, IconName, Tooltip};
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use gpui::{AnyElement, App, FontWeight, IntoElement, Window, div, prelude::*, px};

use crate::ui::glass::{self, PillStyle};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavItem {
    Today,
    NewPath,
    Paths,
    Search,
}

impl NavItem {
    pub const ALL: [NavItem; 4] = [NavItem::Today, NavItem::NewPath, NavItem::Paths, NavItem::Search];

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Today => "Today",
            NavItem::NewPath => "New path",
            NavItem::Paths => "Paths",
            NavItem::Search => "Search",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            NavItem::Today => IconName::House,
            NavItem::NewPath => IconName::Plus,
            NavItem::Paths => IconName::Layers,
            NavItem::Search => IconName::Search,
        }
    }

    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            NavItem::NewPath => Some("Ctrl+N"),
            NavItem::Search => Some("Ctrl+K"),
            _ => None,
        }
    }
}

pub fn brand(cx: &App) -> AnyElement {
    let c = &cx.theme().colors;
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .child(
            div()
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(c.accent)
                .child(Icon::new(IconName::Sparkles).size(IconSize::Sm).color(c.on_accent)),
        )
        .child(
            div()
                .text_size(px(17.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c.fg)
                .child("nuevette"),
        )
        .into_any_element()
}

pub fn pill_nav(
    active: Option<NavItem>,
    on_select: impl Fn(NavItem, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let on_select = Rc::new(on_select);
    let c = &cx.theme().colors;
    div()
        .flex()
        .items_center()
        .gap_1()
        .p_1()
        .rounded_full()
        .bg(c.fg.opacity(0.04))
        .border_1()
        .border_color(c.border_strong)
        .children(NavItem::ALL.map(|item| {
            let handler = on_select.clone();
            let id = ("nav", item as usize);
            let button = if active == Some(item) {
                glass::pill(id, item.label(), Some(item.icon()), PillStyle::Selected, cx)
            } else {
                let tip = match item.shortcut() {
                    Some(keys) => format!("{} ({keys})", item.label()),
                    None => item.label().to_string(),
                };
                glass::round_icon(id, item.icon(), cx).tooltip(Tooltip::text(tip))
            };
            button.on_click(move |_, window, cx| handler(item, window, cx))
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_items_have_labels_and_the_global_shortcuts() {
        assert_eq!(NavItem::ALL.len(), 4);
        assert_eq!(NavItem::NewPath.shortcut(), Some("Ctrl+N"));
        assert_eq!(NavItem::Search.shortcut(), Some("Ctrl+K"));
        assert_eq!(NavItem::Today.label(), "Today");
    }
}
