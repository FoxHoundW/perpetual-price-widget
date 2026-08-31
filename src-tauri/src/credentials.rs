use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "io.github.perpetual-price-widget.proxy";

pub fn save_proxy_password(username: &str, password: &str) -> Result<()> {
    let identity = credential_identity(username);
    let entry = Entry::new(SERVICE, &identity).context("无法打开 Windows 凭据管理器")?;
    if password.is_empty() {
        let _ = entry.delete_credential();
    } else {
        entry.set_password(password).context("无法保存代理凭据")?;
    }
    Ok(())
}

pub fn get_proxy_password(username: &str) -> Option<String> {
    if username.is_empty() {
        return None;
    }
    Entry::new(SERVICE, &credential_identity(username))
        .ok()?
        .get_password()
        .ok()
}

fn credential_identity(username: &str) -> String {
    if username.trim().is_empty() {
        "default".into()
    } else {
        username.trim().to_owned()
    }
}
