//! Settings: profile, appearance, New path defaults, and what the app is
//! configured with (keys are shown as set or missing, never their values),
//! plus exporting and clearing local data.

use std::time::Duration;

use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ActiveTheme;
use ely_gpui_component::typography::Caption;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, IntoElement, Render, SharedString,
    Subscription, Task, Window, div, prelude::*, px,
};

use crate::config::Config;
use crate::model::Level;
use crate::settings::{GlowStrength, Settings, ThemeChoice, data_dir};
use crate::store::PathStore;
use crate::ui::composer::PACES;
use crate::ui::glass::{self, GlassCard, Hue, PillStyle, Segment};

pub enum SettingsEvent {
    /// Every path should go, with a way back.
    ClearData,
    Exported(String),
    ExportFailed(String),
}

pub struct SettingsView {
    store: Entity<PathStore>,
    name: Entity<TextInput>,
    /// Clearing takes a second click while this is set; it lapses on its own.
    confirm_clear: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub fn new(store: Entity<PathStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = cx.global::<Settings>().name.clone();
        let name = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).placeholder("Your name");
            input.set_text(current, cx);
            input
        });
        let _subscriptions = vec![
            cx.subscribe(&name, |_, input, event: &InputEvent, cx| {
                if *event == InputEvent::Changed {
                    let text = input.read(cx).text().trim().to_string();
                    Settings::update(cx, |settings| settings.name = text);
                }
            }),
            cx.observe(&store, |_, _, cx| cx.notify()),
        ];
        Self { store, name, confirm_clear: None, _subscriptions }
    }

    fn row(label: &'static str, hint: Option<&'static str>, control: impl IntoElement, cx: &App) -> AnyElement {
        let c = &cx.theme().colors;
        div()
            .flex()
            .items_center()
            .gap_4()
            .py_1()
            .child(
                div()
                    .flex_1()
                    .min_w(px(140.))
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().font_weight(FontWeight::MEDIUM).child(label))
                    .children(hint.map(|hint| div().text_size(px(13.)).text_color(c.fg_muted).child(hint))),
            )
            .child(control)
            .into_any_element()
    }

    fn status(set: bool, cx: &App) -> AnyElement {
        if set { glass::chip("Set", Hue::Success, cx) } else { glass::chip("Missing", Hue::Neutral, cx) }
            .into_any_element()
    }

    fn export_all(&mut self, cx: &mut Context<Self>) {
        let paths = self.store.read(cx).paths().to_vec();
        let folder = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_default();
        let chosen = cx.prompt_for_new_path(&folder, Some("nuevette-paths.json"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(file))) = chosen.await else {
                return;
            };
            let written = cx
                .background_spawn(async move {
                    let json = serde_json::to_string_pretty(&paths).map_err(|e| e.to_string())?;
                    std::fs::write(&file, json).map_err(|e| e.to_string())?;
                    Ok::<_, String>(file.display().to_string())
                })
                .await;
            this.update(cx, |_, cx| {
                cx.emit(match written {
                    Ok(file) => SettingsEvent::Exported(file),
                    Err(error) => SettingsEvent::ExportFailed(error),
                })
            })
            .ok();
        })
        .detach();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        if self.confirm_clear.take().is_some() {
            cx.emit(SettingsEvent::ClearData);
        } else {
            // Arm for a few seconds, then quietly disarm.
            self.confirm_clear = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(4)).await;
                this.update(cx, |this, cx| {
                    this.confirm_clear = None;
                    cx.notify();
                })
                .ok();
            }));
        }
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<Settings>().clone();
        let config = cx.global::<Config>().clone();
        let c = cx.theme().colors.clone();
        let count = self.store.read(cx).paths().len();

        let theme = glass::segmented(
            "settings-theme",
            vec![
                Segment::new("dark", "Dark").icon(IconName::Moon),
                Segment::new("light", "Light").icon(IconName::Sun),
                Segment::new("system", "System").icon(IconName::Monitor),
            ],
            match settings.theme {
                ThemeChoice::Dark => "dark",
                ThemeChoice::Light => "light",
                ThemeChoice::System => "system",
            },
            |value, window, cx| {
                let choice = match value.as_ref() {
                    "light" => ThemeChoice::Light,
                    "system" => ThemeChoice::System,
                    _ => ThemeChoice::Dark,
                };
                Settings::update(cx, |settings| settings.theme = choice);
                crate::theme::apply(window.appearance(), cx);
            },
            cx,
        );
        let glow = glass::segmented(
            "settings-glow",
            GlowStrength::ALL.iter().map(|g| Segment::new(g.label(), g.label())).collect(),
            settings.glow.label(),
            |value, _, cx| {
                let glow = GlowStrength::ALL.into_iter().find(|g| g.label() == value.as_ref()).unwrap_or_default();
                Settings::update(cx, |settings| settings.glow = glow);
            },
            cx,
        );
        let level = glass::segmented(
            "settings-level",
            Level::ALL.iter().map(|l| Segment::new(l.label(), l.label())).collect(),
            settings.level.label(),
            |value, _, cx| {
                let level = Level::ALL.into_iter().find(|l| l.label() == value.as_ref()).unwrap_or_default();
                Settings::update(cx, |settings| settings.level = level);
            },
            cx,
        );
        let pace_value: SharedString = settings.pace.map_or("any".into(), |h| h.to_string().into());
        let pace = glass::segmented(
            "settings-pace",
            PACES.iter().map(|(value, label)| Segment::new(*value, *label)).collect(),
            &pace_value,
            |value, _, cx| {
                let pace = value.parse().ok();
                Settings::update(cx, |settings| settings.pace = pace);
            },
            cx,
        );

        let initial = settings.initial();
        let profile = GlassCard::new()
            .title("Profile")
            .description("Your name appears in the greeting on Today.")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(52.))
                            .rounded_full()
                            .bg(c.accent.opacity(0.14))
                            .border_1()
                            .border_color(c.accent.opacity(0.5))
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c.accent)
                            .child(initial.map(String::from).unwrap_or_else(|| "?".into())),
                    )
                    .child(div().flex_1().child(Input::new(&self.name))),
            );

        let appearance = GlassCard::new()
            .title("Appearance")
            .child(Self::row("Theme", Some("System follows Windows."), theme, cx))
            .child(Self::row("Backdrop glow", Some("The soft light behind every screen."), glow, cx));

        let defaults = GlassCard::new()
            .title("Learning defaults")
            .description("New path starts with these.")
            .child(Self::row("Level", None, level, cx))
            .child(Self::row("Pace", None, pace, cx));

        let fallback = config.gemini_fallback_model.clone().unwrap_or_else(|| "None".into());
        let folder = data_dir().map(|dir| dir.display().to_string()).unwrap_or_else(|| "Not available".into());
        let mono = cx.theme().mono_family.clone();
        // Values give way (with an ellipsis) before labels do.
        let value = |text: String| {
            div()
                .min_w_0()
                .max_w(px(260.))
                .overflow_hidden()
                .text_ellipsis()
                .font_family(mono.clone())
                .text_size(px(13.))
                .text_color(c.fg_muted)
                .child(text)
        };
        let clear_label = if self.confirm_clear.is_some() { "Click again to delete" } else { "Clear local data" };
        let ai = GlassCard::new()
            .title("AI & data")
            .description("Read from .env at startup. Keys are never shown.")
            .child(Self::row("Model", None, value(config.gemini_model.clone()), cx))
            .child(Self::row("Fallback model", Some("Used when the model is busy."), value(fallback), cx))
            .child(Self::row("Gemini API key", None, Self::status(config.gemini_api_key.is_some(), cx), cx))
            .child(Self::row(
                "SerpAPI key",
                Some("Finds the official docs for a topic."),
                Self::status(config.serpapi_api_key.is_some(), cx),
                cx,
            ))
            .child(Self::row("Data folder", None, value(folder), cx))
            .footer(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .child(Caption::new(format!("{count} path{} saved", if count == 1 { "" } else { "s" })))
                    .child(div().flex_1())
                    .child(
                        glass::pill("settings-export", "Export all paths", Some(IconName::Download), PillStyle::Ghost, cx)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export_all(cx))),
                    )
                    .child({
                        let pill = glass::pill("settings-clear", clear_label, Some(IconName::Trash2), PillStyle::Ghost, cx)
                            .text_color(c.danger);
                        let pill = if self.confirm_clear.is_some() {
                            pill.bg(c.danger.opacity(0.14)).border_color(c.danger.opacity(0.5))
                        } else {
                            pill
                        };
                        if count == 0 {
                            glass::disabled(pill)
                        } else {
                            pill.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear(cx)))
                        }
                    }),
            );

        ScrollArea::new("settings").size_full().child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .px_6()
                .pt_2()
                .pb_5()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_3()
                        .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Settings"))
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(c.fg_muted)
                                .child("Saved on this computer as you change them."),
                        ),
                )
                // Two columns, so everything fits without scrolling on a
                // normal window; the scroll area is only a fallback.
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_4()
                        .child(div().flex_1().min_w_0().flex().flex_col().gap_4().child(profile).child(appearance))
                        .child(div().flex_1().min_w_0().flex().flex_col().gap_4().child(defaults).child(ai)),
                ),
        )
    }
}
