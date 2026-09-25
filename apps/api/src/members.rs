use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use crate::{auth, demo, sites, ApiError, AppState};

#[derive(Debug, Serialize)]
pub(crate) struct SiteMember {
    avatar_url: Option<String>,
    can_remove: bool,
    email: String,
    id: String,
    is_current_user: bool,
    joined_at: String,
    name: Option<String>,
    role: MemberRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MemberRole {
    Admin,
    Owner,
    ReadOnly,
}

impl MemberRole {
    fn from_database(value: &str) -> Self {
        match value {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            _ => Self::ReadOnly,
        }
    }

    fn database_value(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Owner => "owner",
            Self::ReadOnly => "read_only",
        }
    }
}

#[derive(Debug, FromRow)]
struct MemberRow {
    avatar_url: Option<String>,
    email: String,
    id: String,
    joined_at: String,
    name: Option<String>,
    role: String,
}

impl MemberRow {
    fn into_member(self, current_user_id: &str) -> SiteMember {
        let role = MemberRole::from_database(&self.role);
        SiteMember {
            avatar_url: self.avatar_url,
            can_remove: role != MemberRole::Owner && self.id != current_user_id,
            email: self.email,
            is_current_user: self.id == current_user_id,
            id: self.id,
            joined_at: self.joined_at,
            name: self.name,
            role,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddMemberRequest {
    email: String,
    role: WritableMemberRole,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateMemberRequest {
    role: WritableMemberRole,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WritableMemberRole {
    Admin,
    ReadOnly,
}

impl WritableMemberRole {
    fn as_member_role(self) -> MemberRole {
        match self {
            Self::Admin => MemberRole::Admin,
            Self::ReadOnly => MemberRole::ReadOnly,
        }
    }
}

pub(crate) async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
) -> Result<Json<Vec<SiteMember>>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    if auth::is_demo_session(&session) {
        let site = demo::demo_site(&site_identifier)
            .ok_or_else(|| ApiError::Forbidden("You do not have access to this site".to_owned()))?;
        if !sites::demo_site_role(site).can_manage() {
            return Err(ApiError::Forbidden(
                "Site administrator access is required".to_owned(),
            ));
        }
        return Ok(Json(demo_members(site, &session.id)));
    }
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    Ok(Json(load_members(&state.sqlite, &site, &session.id).await?))
}

pub(crate) async fn add_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(site_identifier): Path<String>,
    Json(request): Json<AddMemberRequest>,
) -> Result<Json<SiteMember>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    let email = normalize_email(&request.email)?;
    let user_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE lower(email) = lower(?) AND status = 'active' LIMIT 1",
    )
    .bind(&email)
    .fetch_optional(&state.sqlite)
    .await?
    .ok_or_else(|| {
        ApiError::BadRequest(
            "An instance administrator must create this user under Users first".to_owned(),
        )
    })?;
    protect_owner(&state.sqlite, &site, &user_id).await?;
    upsert_membership(
        &state.sqlite,
        &site,
        &user_id,
        request.role.as_member_role(),
    )
    .await?;
    load_member(&state.sqlite, &site, &user_id, &session.id).await
}

pub(crate) async fn update_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, user_id)): Path<(String, String)>,
    Json(request): Json<UpdateMemberRequest>,
) -> Result<Json<SiteMember>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    if user_id == session.id {
        return Err(ApiError::BadRequest(
            "You cannot change your own site role".to_owned(),
        ));
    }
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    protect_owner(&state.sqlite, &site, &user_id).await?;
    upsert_membership(
        &state.sqlite,
        &site,
        &user_id,
        request.role.as_member_role(),
    )
    .await?;
    load_member(&state.sqlite, &site, &user_id, &session.id).await
}

pub(crate) async fn remove_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((site_identifier, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    auth::reject_demo_workspace_mutation(&session.id)?;
    if user_id == session.id {
        return Err(ApiError::BadRequest(
            "You cannot remove your own site access".to_owned(),
        ));
    }
    let site = sites::resolve_site_for_admin(&state.sqlite, &session.id, &site_identifier).await?;
    protect_owner(&state.sqlite, &site, &user_id).await?;
    let rows = delete_membership(&state.sqlite, &site, &user_id).await?;
    if rows == 0 {
        return Err(ApiError::BadRequest("Unknown site member".to_owned()));
    }
    Ok(Json(serde_json::json!({ "status": "removed" })))
}

async fn load_members(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    current_user_id: &str,
) -> Result<Vec<SiteMember>, ApiError> {
    let mut rows = sqlx::query_as::<_, MemberRow>(
        r#"
        SELECT
            users.id,
            users.email,
            users.name,
            users.avatar_url,
            membership.role,
            membership.created_at AS joined_at
        FROM site_memberships AS membership
        JOIN users ON users.id = membership.user_id
        WHERE membership.site_id = ? AND users.status = 'active'
        ORDER BY
            CASE membership.role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END,
            lower(users.email) ASC
        "#,
    )
    .bind(&site.id)
    .fetch_all(pool)
    .await?;

    // Keep creator ownership recoverable even when a legacy database has not
    // yet materialized its ownership membership row.
    if let Some(creator_id) = site.created_by_user_id.as_deref() {
        rows.retain(|row| row.id != creator_id);
        if let Some(owner) = sqlx::query_as::<_, MemberRow>(
            r#"
            SELECT
                users.id,
                users.email,
                users.name,
                users.avatar_url,
                'owner' AS role,
                sites.created_at AS joined_at
            FROM sites
            JOIN users ON users.id = sites.created_by_user_id
            WHERE sites.id = ? AND users.id = ? AND users.status = 'active'
            LIMIT 1
            "#,
        )
        .bind(&site.id)
        .bind(creator_id)
        .fetch_optional(pool)
        .await?
        {
            rows.push(owner);
        }
    }
    rows.sort_by(|left, right| {
        let role_rank = |role: &str| match role {
            "owner" => 0,
            "admin" => 1,
            _ => 2,
        };
        role_rank(&left.role)
            .cmp(&role_rank(&right.role))
            .then_with(|| left.email.to_lowercase().cmp(&right.email.to_lowercase()))
    });
    Ok(rows
        .into_iter()
        .map(|row| row.into_member(current_user_id))
        .collect())
}

async fn load_member(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user_id: &str,
    current_user_id: &str,
) -> Result<Json<SiteMember>, ApiError> {
    load_members(pool, site, current_user_id)
        .await?
        .into_iter()
        .find(|member| member.id == user_id)
        .map(Json)
        .ok_or_else(|| ApiError::BadRequest("Unknown site member".to_owned()))
}

async fn upsert_membership(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user_id: &str,
    role: MemberRole,
) -> Result<(), ApiError> {
    let role = role.database_value();
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;

    sqlx::query(
        r#"
        INSERT INTO site_memberships (site_id, user_id, role)
        VALUES (?, ?, ?)
        ON CONFLICT(site_id, user_id)
        DO UPDATE SET role = excluded.role, updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(&site.id)
    .bind(user_id)
    .bind(role)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn delete_membership(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user_id: &str,
) -> Result<u64, ApiError> {
    let mut transaction = pool.begin().await?;
    let now = Utc::now().timestamp();
    sqlx::query(
        r#"
        UPDATE dashboard_shares
        SET status = 'revoked', updated_at = ?, responded_at = ?
        WHERE site_id = ? AND user_id = ? AND status <> 'revoked'
        "#,
    )
    .bind(now)
    .bind(now)
    .bind(&site.id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    let rows = sqlx::query("DELETE FROM site_memberships WHERE site_id = ? AND user_id = ?")
        .bind(&site.id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
    transaction.commit().await?;
    Ok(rows)
}

async fn protect_owner(
    pool: &SqlitePool,
    site: &sites::ActiveSite,
    user_id: &str,
) -> Result<(), ApiError> {
    if site.created_by_user_id.as_deref() == Some(user_id) {
        return Err(ApiError::Forbidden(
            "The site owner cannot be changed or removed".to_owned(),
        ));
    }
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM site_memberships WHERE site_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&site.id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    if role.as_deref() == Some("owner") {
        return Err(ApiError::Forbidden(
            "The site owner cannot be changed or removed".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_email(value: &str) -> Result<String, ApiError> {
    auth::normalize_email(value)
}

fn demo_members(site: &demo::DemoSiteDefinition, current_user_id: &str) -> Vec<SiteMember> {
    let joined = (Utc::now() - Duration::days(site.created_days_ago.min(365))).to_rfc3339();
    [
        (
            demo::DEMO_USER_ID,
            demo::PREVIEW_EMAIL,
            demo::PREVIEW_NAME,
            MemberRole::Owner,
        ),
        (
            demo::PREVIEW_MEMBER_MAYA_ID,
            "maya@owleye.dev",
            "Maya Patel",
            MemberRole::Admin,
        ),
        (
            demo::PREVIEW_MEMBER_SAM_ID,
            "sam@example.dev",
            "Sam Rivera",
            MemberRole::ReadOnly,
        ),
    ]
    .into_iter()
    .map(|(id, email, name, role)| SiteMember {
        avatar_url: None,
        can_remove: role != MemberRole::Owner && id != current_user_id,
        email: email.to_owned(),
        id: id.to_owned(),
        is_current_user: id == current_user_id,
        joined_at: joined.clone(),
        name: Some(name.to_owned()),
        role,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::storage::sqlite;

    use super::*;

    #[test]
    fn member_roles_are_deliberately_small() {
        assert_eq!(MemberRole::from_database("owner"), MemberRole::Owner);
        assert_eq!(MemberRole::from_database("admin"), MemberRole::Admin);
        assert_eq!(MemberRole::from_database("member"), MemberRole::ReadOnly);
        assert_eq!(MemberRole::from_database("viewer"), MemberRole::ReadOnly);
    }

    #[test]
    fn email_validation_is_bounded() {
        assert_eq!(
            normalize_email(" USER@Example.com ").unwrap(),
            "user@example.com"
        );
        assert!(normalize_email("missing-at.example.com").is_err());
    }

    #[tokio::test]
    async fn membership_mutations_are_confined_to_one_app() {
        let db_path = std::env::temp_dir().join(format!(
            "owleye-members-{}.sqlite",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pool = sqlite::connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();
        for (id, email) in [
            ("owner", "owner@example.com"),
            ("member", "member@example.com"),
        ] {
            sqlx::query("INSERT INTO users (id, email) VALUES (?, ?)")
                .bind(id)
                .bind(email)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query(
            r#"
            INSERT INTO sites (id, tracking_id, name, domain, created_by_user_id)
            VALUES
                ('site_a', 'owl_a', 'A', 'a.example', 'owner'),
                ('site_b', 'owl_b', 'B', 'b.example', 'owner')
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let site_a = sites::resolve_active_site(&pool, "site_a").await.unwrap();
        let site_b = sites::resolve_active_site(&pool, "site_b").await.unwrap();

        upsert_membership(&pool, &site_a, "member", MemberRole::ReadOnly)
            .await
            .unwrap();
        assert_eq!(
            load_members(&pool, &site_a, "owner").await.unwrap().len(),
            2
        );
        assert_eq!(
            load_members(&pool, &site_b, "owner").await.unwrap().len(),
            1
        );
        assert!(sites::resolve_site_for_user(&pool, "member", "site_a")
            .await
            .is_ok());
        assert!(matches!(
            sites::resolve_site_for_user(&pool, "member", "site_b").await,
            Err(ApiError::Forbidden(_))
        ));

        upsert_membership(&pool, &site_a, "member", MemberRole::Admin)
            .await
            .unwrap();
        assert_eq!(
            sites::effective_site_role(&pool, "member", &site_a)
                .await
                .unwrap(),
            sites::SiteRole::Admin
        );
        assert!(protect_owner(&pool, &site_a, "owner").await.is_err());
        sqlx::query(
            r#"
            INSERT INTO dashboards (
                id, site_id, name, layout, is_default, created_by, created_at, updated_at
            ) VALUES ('shared_dashboard', 'site_a', 'Shared', '[]', 0, 'owner', 1, 1)
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO dashboard_shares (
                dashboard_id, site_id, user_id, invited_by_user_id, status, created_at, updated_at
            ) VALUES ('shared_dashboard', 'site_a', 'member', 'owner', 'accepted', 1, 1)
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            delete_membership(&pool, &site_a, "member").await.unwrap(),
            1
        );
        let share_status: String = sqlx::query_scalar(
            "SELECT status FROM dashboard_shares WHERE dashboard_id = 'shared_dashboard' AND user_id = 'member'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(share_status, "revoked");
        assert!(matches!(
            sites::resolve_site_for_user(&pool, "member", "site_a").await,
            Err(ApiError::Forbidden(_))
        ));

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }
}
