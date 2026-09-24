//! Cross-repository acceptance has its own PMS database lock and Hermes prerequisites.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires owned PMS service through its database test lock"]
async fn real_broker_pms_journey() -> anyhow::Result<()> {
    crate::person_collaboration::business_tests::business_real_broker_pms_journey().await
}
