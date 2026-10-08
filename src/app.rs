//! Root view: the glow, a title bar with the pill nav, a stage (Today,
//! Paths, the composer, or an open path), and a status bar. Toasts confirm
//! deletes (with undo), new paths and map actions.

use ely_gpui_component::feedback::{Toast, ToastViewport, Toaster};
use ely_gpui_component::primitives::{FocusScope, IconName, Severity};
use ely_gpui_component::shell::{StatusBar, StatusBarItem};
use ely_gpui_component::theme::{ActiveTheme, TextSize};
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Render, Subscription,
    Window, div, prelude::*,
};

use crate::actions::{NewPath, OpenPalette};
use crate::config::Config;
use crate::model::LearningPath;
use crate::store::PathStore;
use crate::ui::composer::{ComposerEvent, ComposerView};
use crate::ui::today::{TodayEvent, TodayView};
use crate::ui::mindmap::{MapEvent, MindMapView};
use crate::ui::glass;
use crate::ui::nav::{self, NavItem};
use crate::settings::{Settings, ThemeChoice};
use crate::ui::palette::{Command, Group, PaletteEvent, PaletteView};
use crate::ui::paths::{PathsEvent, PathsView};
use crate::ui::settings_view::{SettingsEvent, SettingsView};
use crate::ui::titlebar;

/// What the stage shows when no path is open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Today,
    Compose,
    Paths,
    Settings,
}

pub struct NuevetteApp {
    store: Entity<PathStore>,
    today: Entity<TodayView>,
    paths: Entity<PathsView>,
    settings: Entity<SettingsView>,
    composer: Entity<ComposerView>,
    screen: Screen,
    toaster: Entity<Toaster>,
    /// The open path and its view; it takes the stage over `screen`.
    mindmap: Option<(String, Entity<MindMapView>)>,
    /// The open command palette, if any.
    palette: Option<Entity<PaletteView>>,
    _palette_sub: Option<Subscription>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    _map_sub: Option<Subscription>,
}

impl NuevetteApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(|_| PathStore::load());
        let today = cx.new(|cx| TodayView::new(store.clone(), cx));
        let paths = cx.new(|cx| PathsView::new(store.clone(), cx));
        let settings = cx.new(|cx| SettingsView::new(store.clone(), window, cx));
        let composer = cx.new(|cx| ComposerView::new(store.clone(), window, cx));
        let toaster = cx.new(|_| Toaster::default());

        let _subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            // "System" follows Windows' light or dark setting as it changes.
            cx.observe_window_appearance(window, |_, window, cx| {
                crate::theme::apply(window.appearance(), cx);
            }),
            cx.subscribe_in(&settings, window, |this, _, event, _, cx| match event {
                SettingsEvent::ClearData => this.clear_data(cx),
                SettingsEvent::Exported(file) => {
                    this.toast(Toast::new("Paths exported").body(file.clone()).severity(Severity::Success), cx)
                }
                SettingsEvent::ExportFailed(error) => {
                    this.toast(Toast::new("Couldn't export").body(error.clone()).severity(Severity::Danger), cx)
                }
            }),
            cx.subscribe_in(&paths, window, |this, _, event, window, cx| match event {
                PathsEvent::Open(id) => this.open_path(id, window, cx),
                PathsEvent::Delete(id) => this.delete_path(id, window, cx),
                PathsEvent::NewPath => this.show(Screen::Compose, window, cx),
                PathsEvent::Import => this.import_paths(cx),
            }),
            cx.subscribe_in(&today, window, |this, _, event, window, cx| match event {
                TodayEvent::Open(id) => this.open_path(id, window, cx),
                TodayEvent::Resume(id, (ti, si)) => {
                    this.open_path(id, window, cx);
                    if let Some((_, map)) = &this.mindmap {
                        map.update(cx, |map, cx| map.select_subtopic(*ti, *si, cx));
                    }
                }
                TodayEvent::NewPath => this.show(Screen::Compose, window, cx),
                TodayEvent::ViewAll => this.show(Screen::Paths, window, cx),
            }),
            cx.subscribe_in(&composer, window, |this, _, event, window, cx| match event {
                ComposerEvent::Started(path) => {
                    let id = path.id.clone();
                    this.store.update(cx, |store, cx| store.add((**path).clone(), cx));
                    this.open_path(&id, window, cx);
                }
                ComposerEvent::Progress(path) => {
                    this.store.update(cx, |store, cx| store.replace_content((**path).clone(), cx));
                }
                ComposerEvent::Finished(path) => {
                    let name = path.name.clone();
                    this.store.update(cx, |store, cx| store.replace_content((**path).clone(), cx));
                    this.toast(
                        Toast::new("Learning path ready")
                            .body(format!("\u{201c}{name}\u{201d} is saved and ready to study."))
                            .severity(Severity::Success),
                        cx,
                    );
                }
                ComposerEvent::Failed(id) => {
                    this.store.update(cx, |store, cx| store.remove(id, cx));
                    // Back to the composer, which shows what went wrong.
                    this.show(Screen::Compose, window, cx);
                }
                ComposerEvent::OpenExisting(id) => this.open_path(id, window, cx),
                ComposerEvent::Cancelled(topic) => this.toast(
                    Toast::new("Generation cancelled")
                        .body(format!("\u{201c}{topic}\u{201d} was not saved.")),
                    cx,
                ),
            }),
        ];

        let focus = cx.focus_handle();
        focus.focus(window, cx);

        Self {
            store,
            today,
            paths,
            settings,
            composer,
            screen: Screen::Today,
            toaster,
            mindmap: None,
            palette: None,
            _palette_sub: None,
            focus,
            _subscriptions,
            _map_sub: None,
        }
    }

    /// Adds paths from shared `.nuevette.json` files. A path you already have
    /// (same id) comes in as a copy, so nothing of yours is overwritten.
    fn import_paths(&mut self, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(files))) = chosen.await else {
                return;
            };
            let read = cx
                .background_spawn(async move {
                    let mut paths = Vec::new();
                    let mut problems = Vec::new();
                    for file in files {
                        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        match std::fs::read_to_string(&file).map_err(anyhow::Error::from).and_then(|t| crate::services::library::import(&t)) {
                            Ok(found) => paths.extend(found),
                            Err(error) => problems.push(format!("{name}: {error:#}")),
                        }
                    }
                    (paths, problems)
                })
                .await;
            this.update(cx, |this, cx| {
                let (paths, problems) = read;
                let count = paths.len();
                this.store.update(cx, |store, cx| {
                    for mut path in paths {
                        if store.get(&path.id).is_some() {
                            path.id = crate::model::new_path_id();
                        }
                        path.building = None;
                        store.add(path, cx);
                    }
                });
                let toast = if problems.is_empty() {
                    Toast::new("Imported")
                        .body(format!("{count} path{} added.", if count == 1 { "" } else { "s" }))
                        .severity(Severity::Success)
                } else {
                    Toast::new(if count == 0 { "Couldn't import" } else { "Imported some files" })
                        .body(problems.join("\n"))
                        .severity(if count == 0 { Severity::Danger } else { Severity::Warning })
                };
                this.toast(toast, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Clears every path at once and offers them back.
    fn clear_data(&mut self, cx: &mut Context<Self>) {
        self.mindmap = None;
        let removed = self.store.update(cx, |store, cx| store.clear(cx));
        let count = removed.len();
        let store = self.store.clone();
        let toast = Toast::new("Local data cleared")
            .body(format!("{count} path{} deleted.", if count == 1 { "" } else { "s" }))
            .undo(move |_, cx| {
                let paths = removed.clone();
                store.update(cx, |store, cx| store.restore_all(paths, cx));
            });
        self.toast(toast, cx);
        cx.notify();
    }

    /// Flips between light and dark. From "System", it pins the opposite of
    /// what is showing.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if cx.theme().is_dark() { ThemeChoice::Light } else { ThemeChoice::Dark };
        Settings::update(cx, |settings| settings.theme = next);
        crate::theme::apply(window.appearance(), cx);
    }

    fn toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toaster.update(cx, |toaster, cx| {
            toaster.push(toast, cx);
        });
    }

    fn open_path(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.read(cx).get(id).is_none() {
            return;
        }
        let store = self.store.clone();
        store.update(cx, |store, cx| store.touch(id, cx));
        let view = cx.new(|cx| MindMapView::new(store, id.to_string(), cx));
        view.focus_handle(cx).focus(window, cx);
        self._map_sub = Some(cx.subscribe(&view, |this, _, event: &MapEvent, cx| {
            let toast = match event {
                MapEvent::StopBuild => {
                    this.composer.update(cx, |composer, cx| composer.cancel(cx));
                    return;
                }
                MapEvent::Done { title, body } => Toast::new(title.clone())
                    .body(body.clone())
                    .severity(Severity::Success),
                MapEvent::Failed { title, body } => Toast::new(title.clone())
                    .body(body.clone())
                    .severity(Severity::Danger),
                MapEvent::Edited { title, undo } => {
                    let (store, before) = (this.store.clone(), (**undo).clone());
                    Toast::new(title.clone()).undo(move |_, cx| {
                        let before = before.clone();
                        store.update(cx, |store, cx| {
                            let id = before.id.clone();
                            store.update_path(&id, cx, |path| *path = before);
                        });
                    })
                }
            };
            this.toast(toast, cx);
        }));
        self.mindmap = Some((id.to_string(), view));
        cx.notify();
    }

    /// Closes any open path and shows Today or the composer.
    fn show(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        self.mindmap = None;
        self.screen = screen;
        if screen == Screen::Compose {
            self.composer.update(cx, |composer, cx| {
                composer.apply_defaults(cx);
                composer.focus_input(window, cx)
            });
        } else {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    /// Deletes at once and offers Undo, rather than asking first.
    fn delete_path(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut removed) = self.store.update(cx, |store, cx| store.remove(id, cx)) else {
            return;
        };
        // Undo brings back what was written so far, no longer being written.
        removed.0.building = None;
        // A path still being written stops being written.
        self.composer.update(cx, |composer, cx| composer.abandon(id, cx));
        if self.mindmap.as_ref().is_some_and(|(open, _)| open == id) {
            self.show(Screen::Today, window, cx);
        }
        let (store, name) = (self.store.clone(), removed.0.name.clone());
        let toast = Toast::new("Path deleted")
            .body(format!("\u{201c}{name}\u{201d} was removed."))
            .undo(move |_, cx| {
                let (path, index) = removed.clone();
                store.update(cx, |store, cx| store.restore(path, index, cx));
            });
        self.toast(toast, cx);
    }

    fn navigate(&mut self, item: NavItem, window: &mut Window, cx: &mut Context<Self>) {
        match item {
            NavItem::Today => self.show(Screen::Today, window, cx),
            NavItem::NewPath => self.show(Screen::Compose, window, cx),
            NavItem::Paths => self.show(Screen::Paths, window, cx),
            NavItem::Settings => self.show(Screen::Settings, window, cx),
            NavItem::Theme => self.toggle_theme(window, cx),
            NavItem::Search => self.open_palette(window, cx),
        }
    }

    /// Runs a command picked in the palette. Values are "kind:argument".
    fn run_command(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (kind, arg) = value.split_once(':').unwrap_or((value, ""));
        let map = self.mindmap.as_ref().map(|(_, map)| map.clone());
        match (kind, arg, map) {
            ("action", "new", _) => self.show(Screen::Compose, window, cx),
            ("action", "today", _) => self.show(Screen::Today, window, cx),
            ("action", "paths", _) => self.show(Screen::Paths, window, cx),
            ("action", "settings", _) => self.show(Screen::Settings, window, cx),
            ("action", "theme", _) => self.toggle_theme(window, cx),
            ("action", "fit", Some(map)) => map.update(cx, |map, cx| map.fit(cx)),
            ("action", "export", Some(map)) => map.update(cx, |map, cx| map.export(cx)),
            ("action", "share", Some(map)) => map.update(cx, |map, cx| map.share(cx)),
            ("action", "import", _) => self.import_paths(cx),
            ("path", id, _) => self.open_path(id, window, cx),
            // A topic ("goto:<path>:<t>") or a step ("goto:<path>:<t>.<s>") in
            // any path: open that path if needed, then select it.
            ("goto", target, _) => {
                let Some((id, place)) = target.rsplit_once(':') else {
                    return;
                };
                if self.mindmap.as_ref().is_none_or(|(open, _)| open != id) {
                    self.open_path(id, window, cx);
                }
                if let Some((_, map)) = &self.mindmap {
                    let step = place.split_once('.').and_then(|(t, s)| Some((t.parse().ok()?, s.parse().ok()?)));
                    match (step, place.parse()) {
                        (Some((t, s)), _) => map.update(cx, |map, cx| map.select_subtopic(t, s, cx)),
                        (None, Ok(t)) => map.update(cx, |map, cx| map.select_topic(t, cx)),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn palette_actions(&self, cx: &App) -> Vec<Command> {
        let dark = cx.theme().is_dark();
        let mut actions = vec![
            Command::new("action:new", "New path", IconName::Plus).keys(&["Ctrl", "N"]),
            Command::new("action:today", "Go to Today", IconName::House),
            Command::new("action:paths", "Go to Paths", IconName::Layers),
            Command::new("action:settings", "Open Settings", IconName::Settings),
            Command::new("action:import", "Import a path file", IconName::Upload),
            Command::new(
                "action:theme",
                if dark { "Switch to light mode" } else { "Switch to dark mode" },
                if dark { IconName::Sun } else { IconName::Moon },
            ),
        ];
        if self.mindmap.is_some() {
            actions.push(Command::new("action:fit", "Fit the map", IconName::Maximize2).keys(&["Ctrl", "0"]));
            actions.push(Command::new("action:export", "Export as Markdown", IconName::Download));
            actions.push(Command::new("action:share", "Share this path as a file", IconName::Share2));
        }
        actions
    }

    /// One path as a palette row, with its progress beside it.
    fn path_command(path: &LearningPath) -> Command {
        let (done, total) = path.progress();
        Command::new(format!("path:{}", path.id), path.name.clone(), IconName::BookOpen)
            .detail(format!("{done} of {total} done"))
    }

    /// Every topic and step of a path, each naming the path it is in.
    fn step_commands(path: &LearningPath) -> Vec<Command> {
        let mut steps = Vec::new();
        for (t, topic) in path.topics.iter().enumerate() {
            steps.push(
                Command::new(format!("goto:{}:{t}", path.id), format!("{}. {}", t + 1, topic.name), IconName::Layers)
                    .detail(path.name.clone()),
            );
            for (s, sub) in topic.subtopics.iter().enumerate() {
                let icon = if path.is_done(t, s) { IconName::CircleCheck } else { IconName::BookOpen };
                steps.push(
                    Command::new(
                        format!("goto:{}:{t}.{s}", path.id),
                        format!("{}.{} {}", t + 1, s + 1, sub.name),
                        icon,
                    )
                    .detail(path.name.clone()),
                );
            }
        }
        steps
    }

    /// What the palette shows before anything is typed (suggestions), and
    /// what it searches once something is (everything).
    fn palette_groups(&self, cx: &App) -> (Vec<Group>, Vec<Group>) {
        let store = self.store.read(cx);
        let open = self.mindmap.as_ref().and_then(|(id, _)| store.get(id));
        let mut recent: Vec<&LearningPath> = store.paths().iter().collect();
        recent.sort_by_key(|p| std::cmp::Reverse(p.last_opened.unwrap_or(0)));

        let continue_steps: Vec<Command> = recent
            .iter()
            .filter_map(|path| {
                let (t, s) = path.next_subtopic()?;
                Some(
                    Command::new(
                        format!("goto:{}:{t}.{s}", path.id),
                        format!("{}.{} {}", t + 1, s + 1, path.topics[t].subtopics[s].name),
                        IconName::ArrowRight,
                    )
                    .detail(path.name.clone()),
                )
            })
            .take(3)
            .collect();

        let mut suggested = Vec::new();
        if !continue_steps.is_empty() {
            suggested.push(Group { title: "Continue".into(), commands: continue_steps });
        }
        suggested.push(Group { title: "Actions".into(), commands: self.palette_actions(cx) });
        if let Some(path) = open {
            suggested.push(Group {
                title: format!("In \u{201c}{}\u{201d}", path.name).into(),
                commands: Self::step_commands(path),
            });
        }
        suggested.push(Group {
            title: "Recent paths".into(),
            commands: recent.iter().take(5).map(|p| Self::path_command(p)).collect(),
        });

        let search = vec![
            Group { title: "Actions".into(), commands: self.palette_actions(cx) },
            Group { title: "Paths".into(), commands: recent.iter().map(|p| Self::path_command(p)).collect() },
            Group {
                title: "Steps".into(),
                // The open path's steps first, then the rest by recency.
                commands: open
                    .into_iter()
                    .chain(recent.iter().copied().filter(|p| Some(p.id.as_str()) != open.map(|o| o.id.as_str())))
                    .flat_map(Self::step_commands)
                    .collect(),
            },
        ];
        (suggested, search)
    }

    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            return;
        }
        let (suggested, search) = self.palette_groups(cx);
        let palette = cx.new(|cx| PaletteView::new(suggested, search, window, cx));
        self._palette_sub = Some(cx.subscribe_in(&palette, window, |this, _, event, window, cx| match event {
            PaletteEvent::Run(value) => {
                let value = value.to_string();
                this.close_palette(window, cx);
                this.run_command(&value, window, cx);
            }
            PaletteEvent::Dismiss => this.close_palette(window, cx),
        }));
        self.palette = Some(palette);
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self._palette_sub = None;
        match &self.mindmap {
            Some((_, map)) => map.focus_handle(cx).focus(window, cx),
            None => self.focus.focus(window, cx),
        }
        cx.notify();
    }

    fn status_bar(&self, cx: &Context<Self>) -> StatusBar {
        let store = self.store.read(cx);
        let config = cx.global::<Config>();
        let saved = match store.problem() {
            Some(problem) => StatusBarItem::new("store")
                .icon(IconName::CircleAlert)
                .label("Not saving")
                .tooltip(problem.to_string()),
            None => StatusBarItem::new("store")
                .icon(IconName::HardDrive)
                .label(format!("{} paths saved locally", store.paths().len())),
        };
        let fallback = config
            .gemini_fallback_model
            .as_deref()
            .map(|model| format!("Falls back to {model} when busy"))
            .unwrap_or_else(|| "No fallback model".into());
        let model = StatusBarItem::new("model")
            .label(config.gemini_model.clone())
            .tooltip(fallback);
        let docs = StatusBarItem::new("docs")
            .icon(IconName::Search)
            .label(if config.serpapi_api_key.is_some() {
                "Docs search on"
            } else {
                "Docs search off"
            });
        StatusBar::new().left(saved).right(docs).right(model)
    }
}

impl Render for NuevetteApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stage = match (&self.mindmap, self.screen) {
            (Some((_, mindmap)), _) => mindmap.clone().into_any_element(),
            (None, Screen::Today) => self.today.clone().into_any_element(),
            (None, Screen::Compose) => self.composer.clone().into_any_element(),
            (None, Screen::Paths) => self.paths.clone().into_any_element(),
            (None, Screen::Settings) => self.settings.clone().into_any_element(),
        };
        let active = match (&self.mindmap, self.screen) {
            (Some(_), _) => None,
            (None, Screen::Today) => Some(NavItem::Today),
            (None, Screen::Compose) => Some(NavItem::NewPath),
            (None, Screen::Paths) => Some(NavItem::Paths),
            (None, Screen::Settings) => Some(NavItem::Settings),
        };
        let status = self.status_bar(cx);
        let palette = self.palette.clone();
        let app = cx.entity().downgrade();
        let initial = cx.global::<Settings>().initial();
        let nav = nav::pill_nav(
            active,
            initial,
            move |item, window, cx| {
                app.update(cx, |app, cx| app.navigate(item, window, cx)).ok();
            },
            cx,
        );
        let brand = nav::brand(cx);
        let title_bar = titlebar::title_bar(brand, nav, window, cx);
        let glow = glass::glow(1., cx);
        let theme = cx.theme();

        FocusScope::new(&self.focus)
            .root()
            .relative()
            .size_full()
            .font_family(theme.font_family.clone())
            .text_size(theme.text_size(TextSize::Base))
            .text_color(theme.colors.fg)
            .bg(theme.colors.bg)
            .child(glow)
            .child(
                div()
                    .key_context("Nuevette")
                    .on_action(cx.listener(|this, _: &NewPath, window, cx| this.show(Screen::Compose, window, cx)))
                    .on_action(cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette(window, cx)))
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(title_bar)
                    .child(div().flex_1().min_h_0().child(stage))
                    .child(status),
            )
            .children(palette)
            .child(ToastViewport::new("toasts", &self.toaster))
    }
}
