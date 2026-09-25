use std::time::Duration as StdDuration;

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::SqlitePool;
use tokio::{sync::watch, time::MissedTickBehavior};

const AUTH_CLEANUP_BATCH_SIZE: u32 = 500;
const AUTH_CLEANUP_INTERVAL: StdDuration = StdDuration::from_secs(15 * 60);

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct AuthCleanupCounts {
    pub(crate) login_challenges: u64,
    pub(crate) sessions: u64,
    pub(crate) step_up_challenges: u64,
}

impl AuthCleanupCounts {
    pub(crate) fn total(&self) -> u64 {
        self.login_challenges + self.sessions + self.step_up_challenges
    }
}

/// Delete at most one small batch per auth table. Startup and periodic callers
/// intentionally do not loop until empty, keeping SQLite lock time bounded.
pub(crate) async fn cleanup_auth_rows(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<AuthCleanupCounts, sqlx::Error> {
    cleanup_auth_rows_with_limit(pool, now, AUTH_CLEANUP_BATCH_SIZE).await
}

pub(crate) async fn run_periodic_auth_cleanup(
    pool: SqlitePool,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval_at(
        tokio::time::Instant::now() + AUTH_CLEANUP_INTERVAL,
        AUTH_CLEANUP_INTERVAL,
    );
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match cleanup_auth_rows(&pool, Utc::now()).await {
                    Ok(counts) if counts.total() > 0 => {
                        tracing::info!(
                            login_challenges = counts.login_challenges,
                            sessions = counts.sessions,
                            step_up_challenges = counts.step_up_challenges,
                            "removed expired authentication records"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%error, "periodic authentication cleanup failed");
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }
    }
}

async fn cleanup_auth_rows_with_limit(
    pool: &SqlitePool,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<AuthCleanupCounts, sqlx::Error> {
    let now = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut transaction = pool.begin().await?;
    let login_challenges = sqlx::query(
        r#"
        DELETE FROM auth_login_challenges
        WHERE rowid IN (
            SELECT rowid FROM auth_login_challenges
            WHERE unixepoch(expires_at) <= unixepoch(?)
            ORDER BY unixepoch(expires_at) ASC, rowid ASC
            LIMIT ?
        )
        "#,
    )
    .bind(&now)
    .bind(limit)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    let step_up_challenges = sqlx::query(
        r#"
        DELETE FROM auth_step_up_challenges
        WHERE rowid IN (
            SELECT rowid FROM auth_step_up_challenges
            WHERE consumed_at IS NOT NULL OR unixepoch(expires_at) <= unixepoch(?)
            ORDER BY unixepoch(COALESCE(consumed_at, expires_at)) ASC, rowid ASC
            LIMIT ?
        )
        "#,
    )
    .bind(&now)
    .bind(limit)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    let sessions = sqlx::query(
        r#"
        DELETE FROM auth_sessions
        WHERE rowid IN (
            SELECT rowid FROM auth_sessions
            WHERE revoked_at IS NOT NULL
                OR unixepoch(COALESCE(refresh_expires_at, expires_at)) <= unixepoch(?)
            ORDER BY unixepoch(COALESCE(revoked_at, refresh_expires_at, expires_at)) ASC, rowid ASC
            LIMIT ?
        )
        "#,
    )
    .bind(&now)
    .bind(limit)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    transaction.commit().await?;

    Ok(AuthCleanupCounts {
        login_challenges,
        sessions,
        step_up_challenges,
    })
}
