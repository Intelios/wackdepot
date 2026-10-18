//! Global git identity — Settings › Git reads and writes `user.name` /
//! `user.email` in the user's global config.

use crate::git::run_global;

/// `(user.name, user.email)` from the global config; `None` where unset.
pub fn global_identity() -> (Option<String>, Option<String>) {
    let get = |key: &str| {
        run_global(["config", "--global", "--get", key])
            .ok()
            .map(|o| o.stdout.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    (get("user.name"), get("user.email"))
}

/// Write `user.name` and `user.email` to the global config.
pub fn set_global_identity(name: &str, email: &str) -> Result<(), String> {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() || email.is_empty() {
        return Err("Couldn't save — both a name and an email are needed.".into());
    }
    if !email.contains('@') {
        return Err("Couldn't save — that email doesn't look right.".into());
    }
    run_global(["config", "--global", "user.name", name])?;
    run_global(["config", "--global", "user.email", email])?;
    Ok(())
}
