//! The learner's preferences: name, look, and New path defaults. Kept as a
//! gpui global and saved to `settings.json` beside `paths.json`.

use std::fs;
use std::path::PathBuf;

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use crate::model::Level;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    /// Follows Windows' light or dark setting, live.
    System,
}

/// How strongly the backdrop glow shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlowStrength {
    Off,
    Soft,
    #[default]
    Full,
}

impl GlowStrength {
    pub const ALL: [GlowStrength; 3] = [GlowStrength::Off, GlowStrength::Soft, GlowStrength::Full];

    pub fn label(self) -> &'static str {
        match self {
            GlowStrength::Off => "Off",
            GlowStrength::Soft => "Soft",
            GlowStrength::Full => "Full",
        }
    }

    pub fn opacity(self) -> f32 {
        match self {
            GlowStrength::Off => 0.,
            GlowStrength::Soft => 0.55,
            GlowStrength::Full => 1.,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Used in the greeting and the nav avatar; empty means not set.
    pub name: String,
    pub theme: ThemeChoice,
    pub glow: GlowStrength,
    /// New path starts with these.
    pub level: Level,
    pub pace: Option<u32>,
}

impl Global for Settings {}

/// Where the app keeps its files: `$NUEVETTE_DATA_DIR` if set (dev scripts),
/// otherwise `nuevette` under the user's data directory.
pub fn data_dir() -> Option<PathBuf> {
    std::env::var_os("NUEVETTE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|dir| dir.join("nuevette")))
}

impl Settings {
    fn file() -> Option<PathBuf> {
        data_dir().map(|dir| dir.join("settings.json"))
    }

    /// The saved settings, or the defaults when there are none (or they can't
    /// be read; a bad file is never fatal).
    pub fn load() -> Self {
        Self::file()
            .and_then(|file| fs::read_to_string(file).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Some(file) = Self::file() else {
            return;
        };
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(error) = fs::write(&file, text) {
                    eprintln!("nuevette: couldn't save settings: {error}");
                }
            }
            Err(error) => eprintln!("nuevette: couldn't encode settings: {error}"),
        }
    }

    /// The first letter of the name, for the avatar.
    pub fn initial(&self) -> Option<char> {
        self.name.trim().chars().next().map(|c| c.to_ascii_uppercase())
    }

    /// Edits, saves and repaints.
    pub fn update(cx: &mut App, edit: impl FnOnce(&mut Settings)) {
        edit(cx.global_mut::<Settings>());
        cx.global::<Settings>().save();
        cx.refresh_windows();
    }
}
