use ely_gpui_component::theme::{Mode, Palette, Theme};
use gpui::{App, Hsla, WindowAppearance, rgba};

use crate::settings::{Settings, ThemeChoice};

const DARK: &[(&str, u32)] = &[
    ("bg", 0x0A0C0FFF),
    ("surface", 0x11151AFF),
    ("sunken", 0x07090BFF),
    ("overlay", 0x141920FF),
    ("hover", 0xFFFFFF0F),
    ("active", 0xFFFFFF17),
    ("border", 0xFFFFFF14),
    ("border_strong", 0xFFFFFF24),
    ("fg", 0xECEFF1FF),
    ("fg_muted", 0x8C959EFF),
    ("fg_subtle", 0x5C656EFF),
    ("accent", 0x2CC3B5FF),
    ("accent_hover", 0x4AD6C8FF),
    ("on_accent", 0x04211FFF),
    ("focus", 0x2CC3B5FF),
    ("link", 0x4AD6C8FF),
    ("success", 0x4ADE80FF),
    ("danger", 0xF06B6BFF),
];

const LIGHT: &[(&str, u32)] = &[
    ("bg", 0xF2F5F5FF),
    ("surface", 0xFFFFFFFF),
    ("sunken", 0xE9EEEEFF),
    ("overlay", 0xFFFFFFFF),
    ("hover", 0x0E1A1C0D),
    ("active", 0x0E1A1C17),
    ("border", 0x0E1A1C17),
    ("border_strong", 0x0E1A1C29),
    ("fg", 0x0E1A1CFF),
    ("fg_muted", 0x55656AFF),
    ("fg_subtle", 0x89979BFF),
    ("accent", 0x0E8F84FF),
    ("accent_hover", 0x0B7C72FF),
    ("on_accent", 0xFFFFFFFF),
    ("focus", 0x0E8F84FF),
    ("link", 0x0B7C72FF),
    ("success", 0x16A34AFF),
    ("danger", 0xD64545FF),
];

fn palette(base: Palette, overrides: &[(&str, u32)]) -> Palette {
    let mut palette = base;
    for (token, hex) in overrides {
        *palette.token_mut(token) = rgba(*hex).into();
    }
    *palette.token_mut("selection") = palette.accent.opacity(0.28);
    palette
}

/// Call once after `ely_gpui_component::init` and loading `Settings`, before
/// the first window opens.
pub fn install(cx: &mut App) {
    Theme::set_palette(Mode::Dark, Some(palette(Palette::dark(false), DARK)), cx);
    Theme::set_palette(Mode::Light, Some(palette(Palette::light(false), LIGHT)), cx);
    // `set_palette` cross-fades from Ely's colors; `set_mode_now` afterwards
    // cancels that and applies ours at once, so the first frame is already ours.
    let mode = resolve(cx.global::<Settings>().theme, cx.window_appearance());
    Theme::set_mode_now(mode, cx);
}

/// The mode a choice shows under the system's current appearance.
pub fn resolve(choice: ThemeChoice, appearance: WindowAppearance) -> Mode {
    match choice {
        ThemeChoice::Dark => Mode::Dark,
        ThemeChoice::Light => Mode::Light,
        ThemeChoice::System => match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
        },
    }
}

pub fn apply(appearance: WindowAppearance, cx: &mut App) {
    let mode = resolve(cx.global::<Settings>().theme, appearance);
    if cx.global::<Theme>().mode() != mode {
        Theme::set_mode(mode, cx);
    }
}

pub fn blend(base: Hsla, over: Hsla, amount: f32) -> Hsla {
    let mut mixed = over.opacity(amount);
    mixed = base.blend(mixed);
    mixed.a = 1.;
    mixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_override_names_a_real_token() {
        for table in [DARK, LIGHT] {
            let mut palette = Palette::dark(false);
            for (token, _) in table {
                let _ = palette.token_mut(token);
            }
        }
    }
}
