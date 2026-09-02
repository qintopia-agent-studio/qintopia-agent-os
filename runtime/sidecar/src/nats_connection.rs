use std::{fs, os::unix::fs::PermissionsExt};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use url::Url;

use crate::config::Cli;

const MAX_NATS_AUTH_FILE_BYTES: u64 = 4_096;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NatsAuthFile {
    version: u8,
    username: String,
    password: String,
}

pub async fn connect(cli: &Cli) -> Result<async_nats::Client> {
    validate_connection_config(cli)?;
    let options = match cli.nats_auth_file.as_deref() {
        Some(path) if !path.trim().is_empty() => {
            let auth = load_auth_file(path)?;
            async_nats::ConnectOptions::with_user_and_password(auth.username, auth.password)
        }
        _ => async_nats::ConnectOptions::new(),
    };
    options
        .connect(&cli.nats_url)
        .await
        .map_err(|_| anyhow!("connect to configured NATS server failed"))
}

pub fn validate_connection_config(cli: &Cli) -> Result<()> {
    let url = Url::parse(&cli.nats_url).context("NATS URL is invalid")?;
    if url.scheme() != "nats" {
        bail!("NATS URL must use the nats scheme");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("NATS URL userinfo is forbidden; use QINTOPIA_SIDECAR_NATS_AUTH_FILE");
    }
    if cli.authenticated_raw_subject == cli.raw_subject
        || cli.authenticated_raw_subject == cli.message_subject
    {
        bail!("authenticated raw NATS subject must be distinct");
    }
    if cli.trust_authenticated_raw_subject
        && cli.nats_auth_file.as_deref().is_none_or(str::is_empty)
    {
        bail!("trusted raw NATS capture requires a consumer auth file");
    }
    Ok(())
}

fn load_auth_file(path: &str) -> Result<NatsAuthFile> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| anyhow!("NATS auth file is unavailable"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_NATS_AUTH_FILE_BYTES
        || metadata.permissions().mode() & 0o027 != 0
    {
        bail!("NATS auth file metadata is invalid");
    }
    let bytes = fs::read(path).map_err(|_| anyhow!("NATS auth file is unreadable"))?;
    if bytes.len() as u64 != metadata.len() {
        bail!("NATS auth file changed while being read");
    }
    let auth: NatsAuthFile =
        serde_json::from_slice(&bytes).map_err(|_| anyhow!("NATS auth file content is invalid"))?;
    if auth.version != 1 || !valid_auth_value(&auth.username) || !valid_auth_value(&auth.password) {
        bail!("NATS auth file content is invalid");
    }
    Ok(auth)
}

fn valid_auth_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use super::load_auth_file;

    #[test]
    fn auth_file_requires_exact_bounded_schema_and_private_metadata() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("consumer.json");
        fs::write(
            &path,
            r#"{"version":1,"username":"consumer","password":"synthetic-secret"}"#,
        )
        .expect("write auth file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))
            .expect("set private permissions");

        let auth = load_auth_file(path.to_str().expect("utf-8 path")).expect("load auth file");
        assert_eq!(auth.username, "consumer");
        assert_eq!(auth.password, "synthetic-secret");

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
            .expect("set public permissions");
        assert!(load_auth_file(path.to_str().expect("utf-8 path")).is_err());
    }

    #[test]
    fn auth_file_rejects_unknown_fields() {
        let directory = tempfile::tempdir().expect("create temp directory");
        let path = directory.path().join("consumer.json");
        fs::write(
            &path,
            r#"{"version":1,"username":"consumer","password":"secret","extra":true}"#,
        )
        .expect("write auth file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .expect("set private permissions");
        assert!(load_auth_file(path.to_str().expect("utf-8 path")).is_err());
    }
}
