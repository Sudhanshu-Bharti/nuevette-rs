//! Root view: the glow, a title bar with the pill nav, a stage (Today,
//! Paths, the composer, or an open path), and a status bar. Toasts confirm
//! deletes (with undo), new paths and map actions.

use ely_gpui_component::feedback::{Toast, ToastViewport, Toaster};
use ely_gpui_component::navigation::{Command, CommandPalette};
use ely_gpui_component::primitives::{FocusScope, IconName, Severity};
use ely_gpui_component::shell::{StatusBar, StatusBarItem};
use ely_gpui_component::theme::{ActiveTheme, TextSize};
use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, Render, SharedString, Subscription,
    Window, div, prelude::*,
};

use crate::actions::{NewPath, OpenPalette};
use crate::config::Config;
use crate::store::PathStore;
use crate::ui::composer::{ComposerEvent, ComposerView};
use crate::ui::today::{TodayEvent, TodayView};
use crate::ui::mindmap::{MapEvent, MindMapView};
use crate::ui::glass;
use crate::ui::nav::{self, NavItem};
use crate::ui::paths::{PathsEvent, PathsView};
use crate::ui::titlebar;

/// What the stage shows when no path is open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Today,
    Compose,
    Paths,
}

pub struct NuevetteApp {
    store: Entity<PathStore>,
    today: Entity<TodayView>,
    paths: Entity<PathsView>,
    composer: Entity<ComposerView>,
    screen: Screen,
    toaster: Entity<Toaster>,
    /// The open path and its view; it takes the stage over `screen`.
    mindmap: Option<(String, Entity<MindMapView>)>,
    palette_open: bool,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    _map_sub: Option<Subscription>,
}

impl NuevetteApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(|_| PathStore::load());
        let today = cx.new(|cx| TodayView::new(store.clone(), cx));
        let paths = cx.new(|cx| PathsView::new(store.clone(), cx));
        let composer = cx.new(|cx| ComposerView::new(window, cx));
        let toaster = cx.new(|_| Toaster::default());

        let _subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(&paths, window, |this, _, event, window, cx| match event {
                PathsEvent::Open(id) => this.open_path(id, window, cx),
                PathsEvent::Delete(id) => this.delete_path(id, window, cx),
                PathsEvent::NewPath => this.show(Screen::Compose, window, cx),
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
            composer,
            screen: Screen::Today,
            toaster,
            mindmap: None,
            palette_open: false,
            focus,
            _subscriptions,
            _map_sub: None,
        }
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
            self.composer.update(cx, |composer, cx| composer.focus_input(window, cx));
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
            NavItem::Search => {
                self.palette_open = true;
                cx.notify();
            }
        }
    }

    /// Runs a command picked in the palette. Values are "kind:argument".
    fn run_command(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_open = false;
        let (kind, arg) = value.split_once(':').unwrap_or((value, ""));
        let map = self.mindmap.as_ref().map(|(_, map)| map.clone());
        match (kind, arg, map) {
            ("action", "new", _) => self.show(Screen::Compose, window, cx),
            ("action", "today", _) => self.show(Screen::Today, window, cx),
            ("action", "paths", _) => self.show(Screen::Paths, window, cx),
            ("action", "fit", Some(map)) => map.update(cx, |map, cx| map.fit(cx)),
            ("action", "export", Some(map)) => map.update(cx, |map, cx| map.export(cx)),
            ("path", id, _) => self.open_path(id, window, cx),
            ("topic", t, Some(map)) => {
                if let Ok(t) = t.parse() {
                    map.update(cx, |map, cx| map.select_topic(t, cx));
                }
            }
            ("sub", ts, Some(map)) => {
                let parsed = ts.split_once('.').and_then(|(t, s)| Some((t.parse().ok()?, s.parse().ok()?)));
                if let Some((t, s)) = parsed {
                    map.update(cx, |map, cx| map.select_subtopic(t, s, cx));
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn palette(&self, cx: &mut Context<Self>) -> CommandPalette {
        let store = self.store.read(cx);
        let open = self.mindmap.as_ref().and_then(|(id, _)| store.get(id));
        let mut actions = vec![
            Command::new("action:new", "New path").icon(IconName::Plus).keys("ctrl-n"),
            Command::new("action:today", "Go to Today").icon(IconName::House),
            Command::new("action:paths", "Go to Paths").icon(IconName::Layers),
        ];
        if open.is_some() {
            actions.push(Command::new("action:fit", "Fit the map").icon(IconName::Maximize2).keys("ctrl-0"));
            actions.push(Command::new("action:export", "Export as Markdown").icon(IconName::Download));
        }
        let paths: Vec<Command> = store
            .paths()
            .iter()
            .map(|p| Command::new(format!("path:{}", p.id), p.name.clone()).icon(IconName::BookOpen))
            .collect();
        let mut palette = CommandPalette::new("palette", {
            let app = cx.entity().downgrade();
            move |_, cx| {
                app.update(cx, |app, cx| {
                    app.palette_open = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .group("Actions", actions);
        if let Some(path) = open {
            let mut steps = Vec::new();
            for (t, topic) in path.topics.iter().enumerate() {
                steps.push(Command::new(format!("topic:{t}"), format!("{}. {}", t + 1, topic.name)).icon(IconName::Layers));
                for (s, sub) in topic.subtopics.iter().enumerate() {
                    let icon = if path.is_done(t, s) { IconName::CircleCheck } else { IconName::BookOpen };
                    steps.push(Command::new(format!("sub:{t}.{s}"), format!("{}.{} {}", t + 1, s + 1, sub.name)).icon(icon));
                }
            }
            palette = palette.group(SharedString::from(format!("In \u{201c}{}\u{201d}", path.name)), steps);
        }
        let app = cx.entity().downgrade();
        palette.group("Paths", paths).on_run(move |value, window, cx| {
            app.update(cx, |app, cx| app.run_command(value, window, cx)).ok();
        })
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
        };
        let active = match (&self.mindmap, self.screen) {
            (Some(_), _) => None,
            (None, Screen::Today) => Some(NavItem::Today),
            (None, Screen::Compose) => Some(NavItem::NewPath),
            (None, Screen::Paths) => Some(NavItem::Paths),
        };
        let status = self.status_bar(cx);
        let palette = self.palette_open.then(|| self.palette(cx));
        let app = cx.entity().downgrade();
        let nav = nav::pill_nav(
            active,
            move |item, window, cx| {
                app.update(cx, |app, cx| app.navigate(item, window, cx)).ok();
            },
            cx,
        );
        let brand = nav::brand(cx);
        let title_bar = titlebar::title_bar(brand, nav, window, cx);
        let theme = cx.theme();

        FocusScope::new(&self.focus)
            .root()
            .relative()
            .size_full()
            .font_family(theme.font_family.clone())
            .text_size(theme.text_size(TextSize::Base))
            .text_color(theme.colors.fg)
            .bg(theme.colors.bg)
            .child(glass::glow(1., cx))
            .child(
                div()
                    .key_context("Nuevette")
                    .on_action(cx.listener(|this, _: &NewPath, window, cx| this.show(Screen::Compose, window, cx)))
                    .on_action(cx.listener(|this, _: &OpenPalette, _, cx| {
                        this.palette_open = true;
                        cx.notify();
                    }))
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
