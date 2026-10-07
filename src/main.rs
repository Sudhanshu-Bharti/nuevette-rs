mod actions;
mod app;
mod assets;
mod config;
mod model;
mod services;
mod settings;
mod stats;
mod store;
mod theme;
mod ui;

use ely_gpui_component::theme::ActiveTheme;
use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use crate::app::NuevetteApp;
use crate::assets::AppAssets;

fn main() {
    gpui_platform::application()
        .with_assets(AppAssets)
        .run(|cx: &mut App| {
            ely_gpui_component::init(cx);
            cx.set_global(settings::Settings::load());
            theme::install(cx);
            actions::init(cx);
            let config = config::Config::load();
            eprintln!("nuevette: {config:?}");
            cx.set_global(config);

            let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Ely's TitleBar draws the title and, on Windows, the caption buttons.
                titlebar: Some(TitlebarOptions {
                    title: Some("Nuevette".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(cx.theme().traffic_light_origin()),
                }),
                window_min_size: Some(size(px(960.), px(620.))),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| cx.new(|cx| NuevetteApp::new(window, cx)))
                .expect("failed to open the main window");

            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
