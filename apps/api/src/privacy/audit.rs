use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// A privacy-safe control-plane audit event.
///
/// Callers must keep `metadata` free of request bodies, network addresses,
/// browser strings, authentication material, and other unnecessary personal
/// data. The canonical audit log is accountability evidence, not a second copy
/// of the data being acted on.
pub(crate) struct AuditEvent<'a> {
    pub(crate) action: &'a str,
    pub(crate) metadata: Value,
    pub(crate) organization_id: Option<&'a str>,
    pub(crate) target_id: Option<&'a str>,
    pub(crate) target_type: Option<&'a str>,
    pub(crate) user_id: Option<&'a str>,
}

pub(crate) async fn record(pool: &SqlitePool, event: AuditEvent<'_>) -> Result<(), sqlx::Error> {
    let mut connection = pool.acquire().await?;
    record_on(&mut connection, event).await
}

pub(crate) async fn record_on(
    connection: &mut SqliteConnection,
    event: AuditEvent<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO audit_log (
            id,
            organization_id,
            user_id,
            action,
            target_type,
            target_id,
            metadata_json,
            created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(event.organization_id)
    .bind(event.user_id)
    .bind(event.action)
    .bind(event.target_type)
    .bind(event.target_id)
    .bind(event.metadata.to_string())
    .bind(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true))
    .execute(connection)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sqlite;

    #[tokio::test]
    async fn canonical_events_store_only_deliberate_metadata() {
        let db_path =
            std::env::temp_dir().join(format!("owleye-privacy-audit-{}.sqlite", Uuid::new_v4()));
        let pool = sqlite::connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, created_at, updated_at) VALUES ('user-a', 'a@example.com', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .unwrap();

        record(
            &pool,
            AuditEvent {
                action: "privacy.account_exported",
                metadata: serde_json::json!({
                    "format": "json",
                    "scope": "account_control_plane"
                }),
                organization_id: None,
                target_id: None,
                target_type: Some("account"),
                user_id: Some("user-a"),
            },
        )
        .await
        .unwrap();

        let (action, target_type, metadata) = sqlx::query_as::<_, (String, String, String)>(
            "SELECT action, target_type, metadata_json FROM audit_log LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(action, "privacy.account_exported");
        assert_eq!(target_type, "account");
        assert_eq!(
            serde_json::from_str::<Value>(&metadata).unwrap(),
            serde_json::json!({
                "format": "json",
                "scope": "account_control_plane"
            })
        );
        assert!(!metadata.contains("a@example.com"));
        assert!(!metadata.contains("ip_address"));
        assert!(!metadata.contains("user_agent"));
    }
}
