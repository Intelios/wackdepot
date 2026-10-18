//! wackdepot-ui — the gpui application: frosted-glass shell, changes/diff/
//! history views, pickers, theme and motion kit.

mod changes;
mod dialogs;
mod diffview;
pub mod frost;
pub mod glass;
mod history;
pub mod icons;
pub mod input;
mod motion;
mod ops;
mod pickers;
pub mod popover;
mod render;
pub mod settings;
pub mod shell;
pub mod state;
pub mod theme;
mod watcher;
mod welcome;

use gpui::{
    App, AppContext as _, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, actions, point, px, size,
};

actions!(
    wackdepot,
    [
        Quit,
        Dismiss,
        ShowChanges,
        ShowHistory,
        ToggleBranches,
        DoFetch,
        DoPull,
        DoPush,
        DoCommit,
        Refresh,
        OpenSettings,
        CloseWindow,
    ]
);

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

/// Open the main window and run the app. Called once from the binary.
pub fn run_app() {
    gpui_platform::application()
        .with_assets(icons::IconAssets)
        .run(|cx: &mut App| {
            cx.on_action(quit);
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-w", CloseWindow, None),
                KeyBinding::new("escape", Dismiss, None),
                KeyBinding::new("cmd-1", ShowChanges, None),
                KeyBinding::new("cmd-2", ShowHistory, None),
                KeyBinding::new("cmd-b", ToggleBranches, None),
                KeyBinding::new("cmd-shift-f", DoFetch, None),
                KeyBinding::new("cmd-enter", DoCommit, None),
                KeyBinding::new("cmd-r", Refresh, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
            ]);
            input::bind_input_keys(cx);
            cx.set_menus(vec![
                Menu::new("WackDepot").items([
                    MenuItem::action("About WackDepot", gpui::NoAction),
                    MenuItem::separator(),
                    MenuItem::action("Settings…", OpenSettings),
                    MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit WackDepot", Quit),
                ]),
                Menu::new("File").items([MenuItem::action("Close Window", CloseWindow)]),
                Menu::new("Repository").items([
                    MenuItem::action("Fetch", DoFetch),
                    MenuItem::action("Pull", DoPull),
                    MenuItem::action("Push", DoPush),
                    MenuItem::separator(),
                    MenuItem::action("Commit", DoCommit),
                    MenuItem::separator(),
                    MenuItem::action("Refresh", Refresh),
                ]),
                Menu::new("View").items([
                    MenuItem::action("Changes", ShowChanges),
                    MenuItem::action("History", ShowHistory),
                ]),
            ]);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Restore the saved window frame when there is one.
            let bounds = match settings::load().window {
                Some(g) => Bounds {
                    origin: point(px(g.x), px(g.y)),
                    size: size(px(g.w), px(g.h)),
                },
                None => Bounds::centered(None, size(px(1100.), px(720.)), cx),
            };
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
                |window, cx| {
                    cx.new(|cx| {
                        let mut shell = shell::Shell::new(cx);
                        shell.observe_geometry(window, cx);
                        shell
                    })
                },
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}
