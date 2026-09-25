use owleye_api::settings::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let command = std::env::args().nth(1);
    if matches!(command.as_deref(), Some("bootstrap" | "recover-admin")) {
        use std::io::Read;
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let input: serde_json::Value = serde_json::from_str(&input)?;
        let pool = owleye_api::storage::sqlite::connect(&std::env::var("SQLITE_URL")?).await?;
        if command.as_deref() == Some("recover-admin") {
            return owleye_api::auth::recover_admin(
                &pool,
                input["email"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("email required"))?,
                input["password"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("password required"))?,
            )
            .await;
        }
        return owleye_api::auth::bootstrap(
            &pool,
            input["email"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("email required"))?,
            input["password"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("password required"))?,
        )
        .await;
    }
    owleye_api::run(Settings::from_env()?).await
}
