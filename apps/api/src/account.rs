use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

use crate::{
    auth, demo,
    privacy::audit::{self, AuditEvent},
    ApiError, AppState,
};

const DELETE_CONFIRMATION: &str = "DELETE MY ACCOUNT";

#[derive(Debug, FromRow, Serialize)]
pub(crate) struct OwnedOrganization {
    id: String,
    name: String,
    slug: String,
    #[sqlx(skip)]
    plan: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProfileSettings {
    name: Option<String>,
    email: String,
    organizations: Vec<OwnedOrganization>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateNameRequest {
    name: String,
}

fn validated_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "Choose a name between 1 and 120 characters, without control characters".to_owned(),
        ));
    }
    Ok(name.to_owned())
}

async fn profile_settings(pool: &SqlitePool, user_id: &str) -> Result<ProfileSettings, ApiError> {
    let (name, email) = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT name, email FROM users WHERE id = ? AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Account is unavailable".to_owned()))?;
    let organizations = vec![];
    Ok(ProfileSettings {
        name,
        email,
        organizations,
    })
}

pub(crate) async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    let mut response = Json(profile_settings(&state.sqlite, &session.id).await?).into_response();
    append_no_store(response.headers_mut());
    Ok(response)
}

pub(crate) async fn update_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateNameRequest>,
) -> Result<Response, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    reject_demo(&session.id)?;
    let name = validated_name(&request.name)?;
    let mut transaction = state.sqlite.begin().await?;
    let result = sqlx::query(
        "UPDATE users SET name = ?, updated_at = ? WHERE id = ? AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(&name)
    .bind(Utc::now().to_rfc3339())
    .bind(&session.id)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError::Unauthorized("Account is unavailable".to_owned()));
    }
    audit::record_on(
        &mut transaction,
        AuditEvent {
            action: "account.profile_updated",
            metadata: serde_json::json!({ "fields": ["name"] }),
            organization_id: None,
            target_id: Some(&session.id),
            target_type: Some("account"),
            user_id: Some(&session.id),
        },
    )
    .await?;
    transaction.commit().await?;
    let mut response = Json(serde_json::json!({ "name": name })).into_response();
    append_no_store(response.headers_mut());
    Ok(response)
}

#[derive(Debug, FromRow, Serialize)]
struct AccountRecord {
    avatar_url: Option<String>,
    created_at: String,
    email: String,
    email_verified_at: Option<String>,
    id: String,
    last_seen_at: Option<String>,
    name: Option<String>,
    two_factor_enabled: bool,
    updated_at: String,
}

#[derive(Debug, FromRow, Serialize)]
struct SiteMembershipRecord {
    created_at: String,
    domain: String,
    id: String,
    name: String,
    role: String,
    tracking_id: String,
}

#[derive(Debug, FromRow, Serialize)]
struct OrganizationMembershipRecord {
    created_at: String,
    id: String,
    name: String,
    role: String,
}

#[derive(Debug, FromRow, Serialize)]
struct SessionRecord {
    created_at: String,
    id: String,
    last_seen_at: Option<String>,
    refresh_reuse_detected_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Debug, FromRow, Serialize)]
struct AuditRecord {
    action: String,
    created_at: String,
    id: String,
    metadata: String,
    organization_id: Option<String>,
    target_id: Option<String>,
    target_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct AccountExport {
    account: AccountRecord,
    audit_records: Vec<AuditRecord>,
    generated_at: String,
    organization_memberships: Vec<OrganizationMembershipRecord>,
    processing_note: &'static str,
    sessions: Vec<SessionRecord>,
    site_memberships: Vec<SiteMembershipRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteAccountRequest {
    confirmation: String,
}

pub(crate) async fn export_account(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    reject_demo(&session.id)?;
    auth::require_recent_step_up(&state.sqlite, &session.session_id, &session.id).await?;

    // Record the successful rights-support action before reading the audit
    // rows so the downloaded package is self-describing. Metadata is kept
    // deliberately categorical: an audit trail must not become a shadow copy
    // of the account export.
    audit::record(
        &state.sqlite,
        AuditEvent {
            action: "privacy.account_exported",
            metadata: serde_json::json!({
                "format": "json",
                "scope": "account_control_plane"
            }),
            organization_id: None,
            target_id: None,
            target_type: Some("account"),
            user_id: Some(&session.id),
        },
    )
    .await?;

    let account = sqlx::query_as::<_, AccountRecord>(
        r#"
        SELECT
            id,
            email,
            email_verified_at,
            name,
            avatar_url,
            created_at,
            updated_at,
            last_seen_at,
            totp_secret IS NOT NULL AS two_factor_enabled
        FROM users
        WHERE id = ? AND status = 'active' AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(&session.id)
    .fetch_optional(&state.sqlite)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Account is unavailable".to_owned()))?;

    let site_memberships = sqlx::query_as::<_, SiteMembershipRecord>(
        r#"
        SELECT
            sites.id,
            sites.tracking_id,
            sites.name,
            sites.domain,
            site_memberships.role,
            site_memberships.created_at
        FROM site_memberships
        INNER JOIN sites ON sites.id = site_memberships.site_id
        WHERE site_memberships.user_id = ?
        ORDER BY sites.created_at ASC, sites.id ASC
        "#,
    )
    .bind(&session.id)
    .fetch_all(&state.sqlite)
    .await?;
    let organization_memberships = sqlx::query_as::<_, OrganizationMembershipRecord>(
        r#"
        SELECT
            organizations.id,
            organizations.name,
            organization_members.role,
            organization_members.created_at
        FROM organization_members
        INNER JOIN organizations ON organizations.id = organization_members.organization_id
        WHERE organization_members.user_id = ?
        ORDER BY organization_members.created_at ASC, organizations.id ASC
        "#,
    )
    .bind(&session.id)
    .fetch_all(&state.sqlite)
    .await?;
    let sessions = sqlx::query_as::<_, SessionRecord>(
        r#"
        SELECT id, created_at, last_seen_at, revoked_at, refresh_reuse_detected_at
        FROM auth_sessions
        WHERE user_id = ?
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(&session.id)
    .fetch_all(&state.sqlite)
    .await?;
    let audit_records = sqlx::query_as::<_, AuditRecord>(
        r#"
        SELECT id, organization_id, action, target_type, target_id, metadata_json AS metadata, created_at
        FROM audit_log
        WHERE user_id = ?
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(&session.id)
    .fetch_all(&state.sqlite)
    .await?;
    let body = serde_json::to_vec_pretty(&AccountExport {
        account,
        audit_records,
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        organization_memberships,
        processing_note: "Account and control-plane data. Visitor analytics are available through the scoped event export.",
        sessions,
        site_memberships,
    })?;
    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=owleye-account-export.json"),
    );
    append_no_store(response.headers_mut());
    Ok(response)
}

pub(crate) async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DeleteAccountRequest>,
) -> Result<Response, ApiError> {
    let session = auth::authenticated_session(&state, &headers).await?;
    reject_demo(&session.id)?;
    auth::require_recent_step_up(&state.sqlite, &session.session_id, &session.id).await?;
    if request.confirmation.trim() != DELETE_CONFIRMATION {
        return Err(ApiError::BadRequest(format!(
            "Type {DELETE_CONFIRMATION} to confirm account deletion"
        )));
    }

    erase_account(&state.sqlite, &session.id, &session.email).await?;

    Ok(auth::cleared_session_json(
        &state,
        serde_json::json!({ "status": "account_deleted" }),
    ))
}

async fn ensure_account_can_be_deleted_on(
    connection: &mut SqliteConnection,
    user_id: &str,
) -> Result<(), ApiError> {
    ensure_account_ownership_clear(connection, user_id).await
}

pub(crate) async fn ensure_account_ownership_clear(
    connection: &mut SqliteConnection,
    user_id: &str,
) -> Result<(), ApiError> {
    let owns_active_site = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM sites
            WHERE sites.archived_at IS NULL
                AND sites.deleted_at IS NULL
                AND (
                    sites.created_by_user_id = ?
                    OR EXISTS(
                        SELECT 1
                        FROM site_memberships
                        WHERE site_memberships.site_id = sites.id
                            AND site_memberships.user_id = ?
                            AND site_memberships.role = 'owner'
                    )
                )
        )
        "#,
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_one(&mut *connection)
    .await?;
    let owns_nonempty_organization = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM organization_members
            INNER JOIN organizations ON organizations.id = organization_members.organization_id
            WHERE organization_members.user_id = ?
                AND organization_members.role = 'owner'
                AND organizations.archived_at IS NULL
                AND (
                    EXISTS(
                        SELECT 1
                        FROM organization_members AS other_members
                        WHERE other_members.organization_id = organizations.id
                            AND other_members.user_id != ?
                            AND EXISTS (SELECT 1 FROM users active_member WHERE active_member.id = other_members.user_id AND active_member.status = 'active' AND active_member.deleted_at IS NULL)
                    )
                    OR EXISTS(
                        SELECT 1
                        FROM sites
                        WHERE sites.organization_id = organizations.id
                            AND sites.archived_at IS NULL
                    )
                )
        )
        "#,
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_one(&mut *connection)
    .await?;
    let owns_legacy_team = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM teams WHERE owner_id = ? AND deleted_at IS NULL)",
    )
    .bind(user_id)
    .fetch_one(&mut *connection)
    .await?;
    if owns_active_site || owns_nonempty_organization || owns_legacy_team {
        return Err(ApiError::Conflict(
            "Transfer or delete every app you own before deleting your account".to_owned(),
        ));
    }
    Ok(())
}

async fn erase_account(pool: &SqlitePool, user_id: &str, email: &str) -> Result<(), ApiError> {
    let mut transaction = pool.begin().await?;
    let admin: bool = sqlx::query_scalar("SELECT is_admin FROM users WHERE id=?")
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await?;
    if admin {
        return Err(ApiError::Conflict(
            "Administrator accounts must be managed by another administrator".into(),
        ));
    }
    // Check ownership in the same transaction as deletion. This closes the
    // gap where a concurrent request could otherwise create an owned resource
    // between validation and erasure and leave it without an owner.
    ensure_account_can_be_deleted_on(&mut transaction, user_id).await?;
    sqlx::query("UPDATE auth_tokens SET consumed_at = COALESCE(consumed_at, unixepoch()) WHERE user_id = ? OR email = ?")
        .bind(user_id)
        .bind(email)
        .execute(&mut *transaction)
        .await?;
    // Keep historical account/audit rows, but remove raw client context.
    sqlx::query("UPDATE audit_logs SET ip_address = NULL, user_agent = NULL WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    // Free-form audit metadata is unnecessary after closure. Preserve the
    // action and timestamp without a second copy of personal profile data.
    sqlx::query(
        r#"
        UPDATE audit_log
        SET
            target_id = CASE
                WHEN target_type IN ('account', 'user') THEN NULL
                ELSE target_id
            END,
            metadata_json = '{"redacted_for_account_erasure":true}'
        WHERE user_id = ?
        "#,
    )
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE exports SET status = 'deleted', download_url = NULL, download_expires_at = NULL WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    // Record the soft deletion without copying personal details into metadata.
    audit::record_on(
        &mut transaction,
        AuditEvent {
            action: "privacy.account_deleted",
            metadata: serde_json::json!({
                "scope": "soft_deleted_account"
            }),
            organization_id: None,
            target_id: None,
            target_type: Some("account"),
            user_id: Some(user_id),
        },
    )
    .await?;

    let result = sqlx::query("UPDATE users SET status = 'deleted', deleted_at = unixepoch(), totp_secret = NULL, password_hash = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND deleted_at IS NULL AND status = 'active'")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError::Unauthorized("Account is unavailable".to_owned()));
    }
    transaction.commit().await?;
    Ok(())
}

fn reject_demo(user_id: &str) -> Result<(), ApiError> {
    if user_id == demo::DEMO_USER_ID {
        Err(ApiError::Forbidden(
            "Account data actions are unavailable in this workspace".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn append_no_store(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(header::EXPIRES, HeaderValue::from_static("0"));
}
