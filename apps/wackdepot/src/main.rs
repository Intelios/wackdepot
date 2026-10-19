//! WackDepot — a native macOS Git GUI.

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--github-credential") => {
            if wackdepot_github::auth::credential_helper(&args.next().unwrap_or_default()).is_err()
            {
                std::process::exit(1);
            }
            return;
        }
        Some("--configure-github-oauth") => {
            if let Err(error) = wackdepot_github::auth::OAuthConfig::configure_from_stdin() {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return;
        }
        Some("--github-export-client") => {
            let flavor = args.next().unwrap_or_default();
            let path = args.next().unwrap_or_default();
            if let Err(error) = wackdepot_github::auth::OAuthConfig::export_registered(
                &flavor,
                std::path::Path::new(&path),
            ) {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return;
        }
        _ => {}
    }
    wackdepot_ui::run_app();
}
