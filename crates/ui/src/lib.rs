//! wackdepot-ui — the gpui application: frosted-glass shell, changes/diff/
//! history views, pickers, theme and motion kit.

pub mod frost;
pub mod glass;
pub mod icons;
pub mod input;
pub mod popover;
pub mod settings;
pub mod shell;
pub mod theme;
mod watcher;

use gpui::{
    App, AppContext as _, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, actions, point, px, size,
};


actions!(wackdepot, [Quit]);

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

/// Open the main window and run the app. Called once from the binary.
pub fn run_app() {
    gpui_platform::application()
        .with_assets(icons::IconAssets)
        .run(|cx: &mut App| {
        cx.on_action(quit);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        input::bind_input_keys(cx);
        cx.set_menus(vec![
            Menu::new("WackDepot").items([
                MenuItem::action("About WackDepot", gpui::NoAction),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit WackDepot", Quit),
            ]),
            Menu::new("File").items([MenuItem::action("Close Window", gpui::NoAction)]),
        ]);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(560.))),
                // Frameless-inset chrome: traffic lights at (14,14), our own
                // 38px strip drags the window (WindowControlArea::Drag +
                // start_window_move), buttons inside it `.occlude()` out of
                // the drag surface.
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.), px(14.))),
                }),
                app_owns_titlebar_drag: true,
                // The whole point: blur the desktop behind the window; shell
                // surfaces paint translucent so the rail reads as frosted
                // glass. (macOS 26+ needs zui's UnderWindowBackground fix.)
                window_background: WindowBackgroundAppearance::Blurred,
                app_id: Some("com.wackdepot.app".into()),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| shell::Shell::new(cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
