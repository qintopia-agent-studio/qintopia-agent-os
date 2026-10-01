//! Manual opt-in validation, outside the automatic PostgreSQL test filters.
#[tokio::test]
#[ignore = "isolated Linux root, separate client users and official Hermes core required"]
async fn actual_production_broker_and_hermes() -> anyhow::Result<()> {
    crate::person_collaboration::steward_production_minimal_tests::linux_production_probe().await
}
