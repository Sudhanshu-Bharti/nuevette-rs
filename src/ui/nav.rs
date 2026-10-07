//! The wordmark and the pill nav in the title bar. The active screen shows
//! as a labelled pill; the rest are round icon buttons with tooltips. After
//! them: the light/dark toggle and the profile avatar, which opens Settings.

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
    /// Flips between light and dark.
    Theme,
    Settings,
}

impl NavItem {
    /// The screens and search, in the pill group.
    pub const ALL: [NavItem; 4] = [NavItem::Today, NavItem::NewPath, NavItem::Paths, NavItem::Search];

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Today => "Today",
            NavItem::NewPath => "New path",
            NavItem::Paths => "Paths",
            NavItem::Search => "Search",
            NavItem::Theme => "Theme",
            NavItem::Settings => "Settings",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            NavItem::Today => IconName::House,
            NavItem::NewPath => IconName::Plus,
            NavItem::Paths => IconName::Layers,
            NavItem::Search => IconName::Search,
            NavItem::Theme => IconName::Sun,
            NavItem::Settings => IconName::Settings,
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
    initial: Option<char>,
    on_select: impl Fn(NavItem, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let on_select = Rc::new(on_select);
    let c = &cx.theme().colors;
    let dark = cx.theme().is_dark();
    let theme_toggle = {
        let handler = on_select.clone();
        let (icon, tip) = if dark { (IconName::Sun, "Light mode") } else { (IconName::Moon, "Dark mode") };
        glass::round_icon("nav-theme", icon, cx)
            .tooltip(Tooltip::text(tip))
            .on_click(move |_, window, cx| handler(NavItem::Theme, window, cx))
    };
    let avatar = {
        let handler = on_select.clone();
        let settings_open = active == Some(NavItem::Settings);
        let ring = if settings_open { c.accent } else { c.border_strong };
        div()
            .id("nav-settings")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(32.))
            .rounded_full()
            .bg(c.accent.opacity(if settings_open { 0.24 } else { 0.14 }))
            .border_1()
            .border_color(ring)
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.accent)
            .cursor_pointer()
            .hover(|s| s.bg(c.accent.opacity(0.22)))
            .tooltip(Tooltip::text("Settings"))
            .map(|avatar| match initial {
                Some(letter) => avatar.child(letter.to_string()),
                None => avatar.child(Icon::new(IconName::User).size(IconSize::Sm).color(c.accent)),
            })
            .on_click(move |_, window, cx| handler(NavItem::Settings, window, cx))
    };
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
        .child(div().w_px().h(px(18.)).mx_1().bg(c.border_strong))
        .child(theme_toggle)
        .child(avatar)
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
