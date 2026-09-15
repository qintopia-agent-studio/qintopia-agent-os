//! Test-only bridge to the owned disposable database harness. Never used by local servers.
use anyhow::{ensure, Result};

pub(crate) fn database_url(prefix: &str) -> Result<String> {
    let local = std::env::var(format!("{prefix}_DATABASE_URL")).ok();
    let input = if let Some(input) = local {
        ensure!(
            std::env::var(format!("{prefix}_ENABLE")).as_deref() == Ok("1"),
            "explicit_test_enable_required"
        );
        input
    } else {
        ensure!(
            std::env::var("QINTOPIA_TEST_MODE").as_deref() == Ok("1")
                && std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref() == Ok("1"),
            "explicit_disposable_harness_required"
        );
        std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("explicit_test_database_required"))?
    };
    normalize(&input)
}

fn normalize(input: &str) -> Result<String> {
    let mut url = url::Url::parse(input).map_err(|_| anyhow::anyhow!("invalid_test_database"))?;
    // The harness emits sslmode=disable. Reject all other query overrides, including
    // host/options, before handing the URL to the unchanged local Store gate.
    ensure!(
        url.query().is_none() || url.query() == Some("sslmode=disable"),
        "test_database_overrides_forbidden"
    );
    url.set_query(None);
    crate::person_collaboration::validate_local_database(url.as_str())?;
    Ok(url.to_string())
}

#[test]
fn disposable_url_accepts_random_ports_but_not_connection_overrides() {
    assert!(normalize(
        "postgres://postgres:postgres@127.0.0.1:55498/qintopia_test?sslmode=disable"
    )
    .is_ok());
    for input in [
        "postgres://postgres@localhost/qintopia_test",
        "postgres://postgres@192.0.2.1/qintopia_test",
        "postgres://postgres@127.0.0.1/production",
        "postgres://postgres@127.0.0.1/qintopia_test?host=192.0.2.1",
        "postgres://postgres@127.0.0.1/qintopia_test?sslmode=disable&options=unsafe",
        "postgres://postgres@127.0.0.1/qintopia_test#override",
    ] {
        assert!(normalize(input).is_err());
    }
}
