//! The rebuilt WackDepot shell: window, backdrop, routing and the views
//! composed over the glass.

use gpui::{
    App, AppContext as _, Bounds, Context, IntoElement, ParentElement, Render, Styled,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, div, point,
    px, size,
};

use crate::icons;

/// Open the main window and run the rebuilt app.
pub fn run() {
    gpui_platform::application()
        .with_assets(icons::IconAssets)
        .run(|cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(900.), px(560.))),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(14.), px(14.))),
                    }),
                    app_owns_titlebar_drag: true,
                    window_background: WindowBackgroundAppearance::Blurred,
                    app_id: Some("com.wackdepot.app".into()),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Workspace),
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}

/// Root view of the window.
pub struct Workspace;

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child("WackDepot")
    }
}
