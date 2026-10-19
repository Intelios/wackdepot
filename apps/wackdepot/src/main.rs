//! WackDepot — a native macOS Git GUI.

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--github-credential") {
        if wackdepot_github::auth::credential_helper(&args.next().unwrap_or_default()).is_err() {
            std::process::exit(1);
        }
        return;
    }
    wackdepot_ui::run_app();
}
