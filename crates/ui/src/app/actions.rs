//! App actions, key bindings (GitHub Desktop's, where it has one) and the
//! macOS menu bar.

use gpui::{App, KeyBinding, Menu, MenuItem, SystemMenuType, actions};

actions!(
    wackdepot,
    [
        Quit,
        CloseWindow,
        About,
        OpenSettings,
        AddRepository,
        CloneRepository,
        NewRepository,
        ShowChanges,
        ShowHistory,
        ShowPullRequests,
        ShowRepositories,
        ShowBranches,
        NewBranch,
        Fetch,
        Pull,
        Push,
        Commit,
        Refresh,
        StashAll,
        OpenInEditor,
        OpenInTerminal,
        ShowInFinder,
        ToggleSplitDiff,
        ToggleWhitespace,
        Dismiss,
    ]
);

pub fn register(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    });
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-o", AddRepository, None),
        KeyBinding::new("cmd-shift-o", CloneRepository, None),
        KeyBinding::new("cmd-n", NewRepository, None),
        KeyBinding::new("cmd-1", ShowChanges, None),
        KeyBinding::new("cmd-2", ShowHistory, None),
        KeyBinding::new("cmd-3", ShowPullRequests, None),
        KeyBinding::new("cmd-t", ShowRepositories, None),
        KeyBinding::new("cmd-b", ShowBranches, None),
        KeyBinding::new("cmd-shift-n", NewBranch, None),
        KeyBinding::new("cmd-shift-t", Fetch, None),
        KeyBinding::new("cmd-shift-p", Pull, None),
        KeyBinding::new("cmd-p", Push, None),
        KeyBinding::new("cmd-enter", Commit, None),
        KeyBinding::new("cmd-r", Refresh, None),
        KeyBinding::new("cmd-alt-s", StashAll, None),
        KeyBinding::new("cmd-shift-a", OpenInEditor, None),
        KeyBinding::new("ctrl-`", OpenInTerminal, None),
        KeyBinding::new("cmd-shift-f", ShowInFinder, None),
        KeyBinding::new("cmd-shift-d", ToggleSplitDiff, None),
        KeyBinding::new("escape", Dismiss, None),
    ]);
    cx.set_menus(vec![
        Menu::new("WackDepot").items([
            MenuItem::action("About WackDepot", About),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Quit WackDepot", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New Repository…", NewRepository),
            MenuItem::action("Add Local Repository…", AddRepository),
            MenuItem::action("Clone Repository…", CloneRepository),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("View").items([
            MenuItem::action("Changes", ShowChanges),
            MenuItem::action("History", ShowHistory),
            MenuItem::action("Pull Requests", ShowPullRequests),
            MenuItem::separator(),
            MenuItem::action("Repositories", ShowRepositories),
            MenuItem::action("Branches", ShowBranches),
            MenuItem::separator(),
            MenuItem::action("Split Diff", ToggleSplitDiff),
            MenuItem::action("Hide Whitespace Changes", ToggleWhitespace),
        ]),
        Menu::new("Repository").items([
            MenuItem::action("Fetch", Fetch),
            MenuItem::action("Pull", Pull),
            MenuItem::action("Push", Push),
            MenuItem::separator(),
            MenuItem::action("Stash All Changes", StashAll),
            MenuItem::separator(),
            MenuItem::action("Open in Editor", OpenInEditor),
            MenuItem::action("Open in Terminal", OpenInTerminal),
            MenuItem::action("Show in Finder", ShowInFinder),
            MenuItem::separator(),
            MenuItem::action("Refresh", Refresh),
        ]),
        Menu::new("Branch").items([
            MenuItem::action("New Branch…", NewBranch),
            MenuItem::action("Switch Branch…", ShowBranches),
            MenuItem::separator(),
            MenuItem::action("Commit", Commit),
        ]),
    ]);
}
