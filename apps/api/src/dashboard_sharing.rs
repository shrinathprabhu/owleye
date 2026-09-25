use std::collections::HashSet;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};

use crate::{auth, dashboards, demo, sites, ApiError, AppState};

const MAX_SHARE_RECIPIENTS: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DashboardShareStatus {
    Accepted,
    Dismissed,
    Pending,
    Revoked,
}

impl DashboardShareStatus {
    pub(crate) fn from_database(value: &str) -> Result<Self, ApiError> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "dismissed" => Ok(Self::Dismissed),
            "pending" => Ok(Self::Pending),
            "revoked" => Ok(Self::Revoked),
            _ => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown dashboard share status: {value}"
            ))),
        }
    }

    fn database_value(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Dismissed => "dismissed",
            Self::Pending => "pending",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DashboardShare {
    created_at: String,
    dashboard_id: String,
    display_name: Option<String>,
    email: String,
    invited_by_user_id: String,
    responded_at: Option<String>,
    status: DashboardShareStatus,
    updated_at: String,
    user_id: String,
}

#[derive(Debug, FromRow)]
struct DashboardShareRow {
    created_at: i64,
    dashboard_id: String,
    display_name: Option<String>,
    email: String,
    invited_by_user_id: String,
    responded_at: Option<i64>,
    status: String,
    updated_at: i64,
    user_id: String,
}

impl DashboardShareRow {
    fn into_share(self) -> Result<DashboardShare, ApiError> {
        Ok(DashboardShare {
            created_at: timestamp(self.created_at),
            dashboard_id: self.dashboard_id,
            display_name: self.display_name,
            email: self.email,
            invited_by_user_id: self.invited_by_user_id,
            responded_at: self.responded_at.map(timestamp),
            status: DashboardShareStatus::from_database(&self.status)?,
            updated_at: timestamp(self.updated_at),
            user_id: self.user_id,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ShareDashboardRequest {
    user_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ShareStateResponse {
    dashboard_id: String,
    status: DashboardShareStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RevokeSharesResponse {
    dashboard_id: String,
    revoked_count: u64,
    scope: &'static str,
}

pub(crate) async fn list_dashboard_invitations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<Vec<dashboards::DashboardSummary>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Ok(Json(Vec::new()));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    dashboards::pending_invitation_summaries(&state.sqlite, &site.id, &session.id)
        .await
        .map(Json)
}

pub(crate) async fn preview_dashboard_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<dashboards::DashboardDetail>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        return Err(ApiError::BadRequest(
            "Unknown dashboard invitation".to_owned(),
        ));
    }
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    dashboards::load_dashboard_detail(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        dashboards::DashboardAccess::PendingInvitation,
    )
    .await
    .map(Json)
}

pub(crate) async fn accept_dashboard_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<dashboards::DashboardDetail>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    transition_recipient_share(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        DashboardShareStatus::Pending,
        DashboardShareStatus::Accepted,
    )
    .await?;
    dashboards::load_dashboard_detail(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        dashboards::DashboardAccess::OwnedOrAccepted,
    )
    .await
    .map(Json)
}

pub(crate) async fn dismiss_dashboard_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<ShareStateResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    transition_recipient_share(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        DashboardShareStatus::Pending,
        DashboardShareStatus::Dismissed,
    )
    .await?;
    Ok(Json(ShareStateResponse {
        dashboard_id,
        status: DashboardShareStatus::Dismissed,
        user_id: None,
    }))
}

pub(crate) async fn list_dashboard_shares(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<Vec<DashboardShare>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    dashboards::ensure_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    load_dashboard_shares(&state.sqlite, &site.id, &dashboard_id)
        .await
        .map(Json)
}

pub(crate) async fn share_dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
    Json(request): Json<ShareDashboardRequest>,
) -> Result<Json<Vec<DashboardShare>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    invite_dashboard_members(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        request.user_ids,
    )
    .await
    .map(Json)
}

pub(crate) async fn remove_dashboard_for_me(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<ShareStateResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    transition_recipient_share(
        &state.sqlite,
        &site.id,
        &dashboard_id,
        &session.id,
        DashboardShareStatus::Accepted,
        DashboardShareStatus::Dismissed,
    )
    .await?;
    Ok(Json(ShareStateResponse {
        dashboard_id,
        status: DashboardShareStatus::Dismissed,
        user_id: None,
    }))
}

pub(crate) async fn revoke_dashboard_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id, user_id)): Path<(String, String, String)>,
) -> Result<Json<ShareStateResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    dashboards::ensure_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    let affected =
        revoke_dashboard_recipients(&state.sqlite, &site.id, &dashboard_id, Some(&user_id)).await?;
    if affected == 0 {
        return Err(ApiError::BadRequest(
            "Unknown dashboard share recipient".to_owned(),
        ));
    }
    Ok(Json(ShareStateResponse {
        dashboard_id,
        status: DashboardShareStatus::Revoked,
        user_id: Some(user_id),
    }))
}

pub(crate) async fn revoke_all_dashboard_shares(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, dashboard_id)): Path<(String, String)>,
) -> Result<Json<RevokeSharesResponse>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_user(&state.sqlite, &session.id, &site_identifier).await?;
    dashboards::ensure_dashboard_owner(&state.sqlite, &site.id, &dashboard_id, &session.id).await?;
    let revoked_count =
        revoke_dashboard_recipients(&state.sqlite, &site.id, &dashboard_id, None).await?;
    Ok(Json(RevokeSharesResponse {
        dashboard_id,
        revoked_count,
        scope: "others",
    }))
}

pub(crate) async fn load_dashboard_shares(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
) -> Result<Vec<DashboardShare>, ApiError> {
    let rows = sqlx::query_as::<_, DashboardShareRow>(
        r#"
        SELECT
            dashboard_shares.dashboard_id,
            dashboard_shares.user_id,
            users.email,
            users.name AS display_name,
            dashboard_shares.invited_by_user_id,
            dashboard_shares.status,
            dashboard_shares.created_at,
            dashboard_shares.updated_at,
            dashboard_shares.responded_at
        FROM dashboard_shares
        JOIN users ON users.id = dashboard_shares.user_id
        WHERE dashboard_shares.dashboard_id = ? AND dashboard_shares.site_id = ?
        ORDER BY
            CASE dashboard_shares.status
                WHEN 'accepted' THEN 0
                WHEN 'pending' THEN 1
                WHEN 'dismissed' THEN 2
                ELSE 3
            END,
            lower(users.email) ASC
        "#,
    )
    .bind(dashboard_id)
    .bind(site_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(DashboardShareRow::into_share)
        .collect()
}

async fn invite_dashboard_members(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
    creator_id: &str,
    recipient_ids: Vec<String>,
) -> Result<Vec<DashboardShare>, ApiError> {
    dashboards::ensure_dashboard_owner(pool, site_id, dashboard_id, creator_id).await?;
    let recipient_ids = normalize_recipient_ids(recipient_ids, creator_id)?;
    ensure_active_site_members(pool, site_id, &recipient_ids).await?;

    let now = Utc::now().timestamp();
    let mut transaction = pool.begin().await?;
    for recipient_id in recipient_ids {
        sqlx::query(
            r#"
            INSERT INTO dashboard_shares (
                dashboard_id, site_id, user_id, invited_by_user_id, status,
                created_at, updated_at, responded_at
            ) VALUES (?, ?, ?, ?, 'pending', ?, ?, NULL)
            ON CONFLICT(dashboard_id, user_id) DO UPDATE SET
                invited_by_user_id = CASE
                    WHEN dashboard_shares.status = 'accepted'
                        THEN dashboard_shares.invited_by_user_id
                    ELSE excluded.invited_by_user_id
                END,
                status = CASE
                    WHEN dashboard_shares.status = 'accepted' THEN 'accepted'
                    ELSE 'pending'
                END,
                updated_at = CASE
                    WHEN dashboard_shares.status = 'accepted'
                        THEN dashboard_shares.updated_at
                    ELSE excluded.updated_at
                END,
                responded_at = CASE
                    WHEN dashboard_shares.status = 'accepted'
                        THEN dashboard_shares.responded_at
                    ELSE NULL
                END
            "#,
        )
        .bind(dashboard_id)
        .bind(site_id)
        .bind(recipient_id)
        .bind(creator_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    load_dashboard_shares(pool, site_id, dashboard_id).await
}

async fn transition_recipient_share(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
    user_id: &str,
    from: DashboardShareStatus,
    to: DashboardShareStatus,
) -> Result<(), ApiError> {
    let now = Utc::now().timestamp();
    let affected = sqlx::query(
        r#"
        UPDATE dashboard_shares
        SET status = ?, updated_at = ?, responded_at = ?
        WHERE dashboard_id = ?
            AND site_id = ?
            AND user_id = ?
            AND status = ?
            AND EXISTS (
                SELECT 1 FROM dashboards
                WHERE dashboards.id = dashboard_shares.dashboard_id
                    AND dashboards.site_id = dashboard_shares.site_id
            )
            AND EXISTS (
                SELECT 1 FROM site_memberships
                WHERE site_memberships.site_id = dashboard_shares.site_id
                    AND site_memberships.user_id = dashboard_shares.user_id
            )
        "#,
    )
    .bind(to.database_value())
    .bind(now)
    .bind(now)
    .bind(dashboard_id)
    .bind(site_id)
    .bind(user_id)
    .bind(from.database_value())
    .execute(pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(ApiError::BadRequest(
            "Dashboard invitation is unavailable or already handled".to_owned(),
        ));
    }
    Ok(())
}

async fn revoke_dashboard_recipients(
    pool: &SqlitePool,
    site_id: &str,
    dashboard_id: &str,
    user_id: Option<&str>,
) -> Result<u64, ApiError> {
    let now = Utc::now().timestamp();
    let mut query = QueryBuilder::<Sqlite>::new(
        "UPDATE dashboard_shares SET status = 'revoked', updated_at = ",
    );
    query
        .push_bind(now)
        .push(", responded_at = ")
        .push_bind(now)
        .push(" WHERE site_id = ")
        .push_bind(site_id)
        .push(" AND dashboard_id = ")
        .push_bind(dashboard_id)
        .push(" AND status <> 'revoked'");
    if let Some(user_id) = user_id {
        query.push(" AND user_id = ").push_bind(user_id);
    }
    Ok(query.build().execute(pool).await?.rows_affected())
}

fn normalize_recipient_ids(
    recipient_ids: Vec<String>,
    creator_id: &str,
) -> Result<Vec<String>, ApiError> {
    if recipient_ids.is_empty() || recipient_ids.len() > MAX_SHARE_RECIPIENTS {
        return Err(ApiError::BadRequest(format!(
            "Choose between 1 and {MAX_SHARE_RECIPIENTS} dashboard recipients"
        )));
    }
    let mut unique = HashSet::with_capacity(recipient_ids.len());
    let mut normalized = Vec::with_capacity(recipient_ids.len());
    for recipient_id in recipient_ids {
        let recipient_id = recipient_id.trim();
        if recipient_id.is_empty()
            || recipient_id.len() > 128
            || recipient_id.chars().any(char::is_control)
        {
            return Err(ApiError::BadRequest(
                "Dashboard recipient ids are invalid".to_owned(),
            ));
        }
        if recipient_id == creator_id {
            return Err(ApiError::BadRequest(
                "The dashboard creator already has access".to_owned(),
            ));
        }
        if unique.insert(recipient_id.to_owned()) {
            normalized.push(recipient_id.to_owned());
        }
    }
    Ok(normalized)
}

async fn ensure_active_site_members(
    pool: &SqlitePool,
    site_id: &str,
    recipient_ids: &[String],
) -> Result<(), ApiError> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT users.id FROM site_memberships \
         JOIN users ON users.id = site_memberships.user_id \
         WHERE site_memberships.site_id = ",
    );
    query
        .push_bind(site_id)
        .push(" AND users.status = 'active' AND users.id IN (");
    let mut separated = query.separated(", ");
    for recipient_id in recipient_ids {
        separated.push_bind(recipient_id);
    }
    separated.push_unseparated(")");
    let eligible = query
        .build_query_scalar::<String>()
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect::<HashSet<_>>();
    if recipient_ids
        .iter()
        .any(|recipient_id| !eligible.contains(recipient_id))
    {
        return Err(ApiError::BadRequest(
            "Every dashboard recipient must be an active member of this site".to_owned(),
        ));
    }
    Ok(())
}

fn timestamp(value: i64) -> String {
    chrono::DateTime::from_timestamp(value, 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::storage::sqlite;

    use super::*;

    #[tokio::test]
    async fn sharing_lifecycle_is_site_scoped_and_read_only_for_recipients() {
        let (pool, db_path) = test_pool().await;
        seed_sharing_fixture(&pool).await;

        assert!(invite_dashboard_members(
            &pool,
            "site_a",
            "dashboard_a",
            "owner",
            vec!["outsider".to_owned()],
        )
        .await
        .is_err());
        assert!(invite_dashboard_members(
            &pool,
            "site_b",
            "dashboard_a",
            "owner",
            vec!["member".to_owned()],
        )
        .await
        .is_err());

        let shares = invite_dashboard_members(
            &pool,
            "site_a",
            "dashboard_a",
            "owner",
            vec!["member".to_owned(), "member".to_owned()],
        )
        .await
        .unwrap();
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].status, DashboardShareStatus::Pending);
        assert_eq!(
            dashboards::pending_invitation_summaries(&pool, "site_a", "member")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(dashboards::load_dashboard_detail(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            dashboards::DashboardAccess::OwnedOrAccepted,
        )
        .await
        .is_err());
        let preview = dashboards::load_dashboard_detail(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            dashboards::DashboardAccess::PendingInvitation,
        )
        .await
        .unwrap();
        let preview_json = serde_json::to_value(preview).unwrap();
        assert_eq!(preview_json["share_status"], "pending");
        assert_eq!(preview_json["capabilities"]["can_edit"], false);

        transition_recipient_share(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            DashboardShareStatus::Pending,
            DashboardShareStatus::Accepted,
        )
        .await
        .unwrap();
        let accepted = dashboards::load_dashboard_detail(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            dashboards::DashboardAccess::OwnedOrAccepted,
        )
        .await
        .unwrap();
        let accepted_json = serde_json::to_value(accepted).unwrap();
        assert_eq!(accepted_json["share_status"], "accepted");
        assert_eq!(accepted_json["capabilities"]["can_remove_for_me"], true);
        assert!(
            dashboards::ensure_dashboard_owner(&pool, "site_a", "dashboard_a", "member")
                .await
                .is_err()
        );

        transition_recipient_share(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            DashboardShareStatus::Accepted,
            DashboardShareStatus::Dismissed,
        )
        .await
        .unwrap();
        assert!(dashboards::load_dashboard_detail(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            dashboards::DashboardAccess::OwnedOrAccepted,
        )
        .await
        .is_err());

        invite_dashboard_members(
            &pool,
            "site_a",
            "dashboard_a",
            "owner",
            vec!["member".to_owned()],
        )
        .await
        .unwrap();
        transition_recipient_share(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            DashboardShareStatus::Pending,
            DashboardShareStatus::Accepted,
        )
        .await
        .unwrap();
        assert_eq!(
            revoke_dashboard_recipients(&pool, "site_a", "dashboard_a", None)
                .await
                .unwrap(),
            1
        );
        assert!(dashboards::load_dashboard_detail(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            dashboards::DashboardAccess::OwnedOrAccepted,
        )
        .await
        .is_err());

        sqlx::query("DELETE FROM dashboards WHERE id = 'dashboard_a'")
            .execute(&pool)
            .await
            .unwrap();
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM dashboard_shares WHERE dashboard_id = 'dashboard_a'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn dismissed_invites_can_be_reissued_but_accepted_shares_are_idempotent() {
        let (pool, db_path) = test_pool().await;
        seed_sharing_fixture(&pool).await;
        invite_dashboard_members(
            &pool,
            "site_a",
            "dashboard_a",
            "owner",
            vec!["member".to_owned()],
        )
        .await
        .unwrap();
        transition_recipient_share(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            DashboardShareStatus::Pending,
            DashboardShareStatus::Dismissed,
        )
        .await
        .unwrap();
        let reissued = invite_dashboard_members(
            &pool,
            "site_a",
            "dashboard_a",
            "owner",
            vec!["member".to_owned()],
        )
        .await
        .unwrap();
        assert_eq!(reissued[0].status, DashboardShareStatus::Pending);
        transition_recipient_share(
            &pool,
            "site_a",
            "dashboard_a",
            "member",
            DashboardShareStatus::Pending,
            DashboardShareStatus::Accepted,
        )
        .await
        .unwrap();
        let still_accepted = invite_dashboard_members(
            &pool,
            "site_a",
            "dashboard_a",
            "owner",
            vec!["member".to_owned()],
        )
        .await
        .unwrap();
        assert_eq!(still_accepted[0].status, DashboardShareStatus::Accepted);

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    async fn test_pool() -> (SqlitePool, std::path::PathBuf) {
        let db_path = std::env::temp_dir().join(format!(
            "owleye-dashboard-sharing-{}.sqlite",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pool = sqlite::connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();
        (pool, db_path)
    }

    async fn seed_sharing_fixture(pool: &SqlitePool) {
        for (id, email) in [
            ("owner", "owner@example.com"),
            ("member", "member@example.com"),
            ("outsider", "outsider@example.com"),
        ] {
            sqlx::query("INSERT INTO users (id, email) VALUES (?, ?)")
                .bind(id)
                .bind(email)
                .execute(pool)
                .await
                .unwrap();
        }
        sqlx::query(
            r#"
            INSERT INTO sites (id, tracking_id, name, domain, created_by_user_id)
            VALUES
                ('site_a', 'owl_share_a', 'A', 'a.example', 'owner'),
                ('site_b', 'owl_share_b', 'B', 'b.example', 'owner')
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO site_memberships (site_id, user_id, role)
            VALUES ('site_a', 'owner', 'owner'), ('site_a', 'member', 'read_only'),
                   ('site_b', 'owner', 'owner')
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO dashboards (
                id, site_id, name, layout, is_default, created_by, created_at, updated_at
            ) VALUES ('dashboard_a', 'site_a', 'Campaign', '[]', 0, 'owner', 1, 1)
            "#,
        )
        .execute(pool)
        .await
        .unwrap();
    }
}
