use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::{str::FromStr, time::Duration};

pub async fn connect(sqlite_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(sqlite_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(16)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .connect_with(options)
        .await?;

    initialize(&pool).await?;
    let columns = sqlx::query("PRAGMA table_info(users)")
        .fetch_all(&pool)
        .await?;
    for (name, definition) in [
        ("password_hash", "TEXT"),
        ("is_admin", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns
            .iter()
            .any(|row| row.get::<String, _>("name") == name)
        {
            sqlx::query(&format!("ALTER TABLE users ADD COLUMN {name} {definition}"))
                .execute(&pool)
                .await?;
        }
    }

    Ok(pool)
}

/// Update the small SQLite event catalog in one transaction after ClickHouse
/// accepts an ingestion batch. The catalog is advisory metadata, but batching
/// avoids turning a 100-event SDK request into 100 SQLite commits.
pub async fn record_event_catalog_batch(
    pool: &SqlitePool,
    events: &[CatalogEvent<'_>],
) -> anyhow::Result<()> {
    if events.is_empty() {
        return Ok(());
    }

    let mut transaction = pool.begin().await?;
    for event in events {
        sqlx::query(
            r#"
        INSERT INTO site_event_catalog (
            site_id,
            event_type,
            event_name,
            first_seen_at,
            last_seen_at,
            total_seen
        )
        VALUES (?, ?, ?, ?, ?, 1)
        ON CONFLICT(site_id, event_type, event_name)
        DO UPDATE SET
            first_seen_at = MIN(site_event_catalog.first_seen_at, excluded.first_seen_at),
            last_seen_at = MAX(site_event_catalog.last_seen_at, excluded.last_seen_at),
            total_seen = site_event_catalog.total_seen + 1
        "#,
        )
        .bind(event.site_id)
        .bind(event.event_type)
        .bind(event.event_name)
        .bind(event.occurred_at)
        .bind(event.occurred_at)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;

    Ok(())
}

pub struct CatalogEvent<'a> {
    pub event_name: &'a str,
    pub event_type: &'a str,
    pub occurred_at: &'a str,
    pub site_id: &'a str,
}

async fn initialize(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    let version =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM app_schema_migrations")
            .fetch_one(pool)
            .await?;
    if version > CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "SQLite schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
        );
    }
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    apply_baseline_schema(pool).await?;
    sqlx::query("INSERT INTO app_schema_migrations (version, name) VALUES (?, ?)")
        .bind(CURRENT_SCHEMA_VERSION)
        .bind("self_hosted_baseline")
        .execute(pool)
        .await?;
    Ok(())
}

async fn apply_baseline_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    for statement in SQLITE_TABLES {
        sqlx::query(statement).execute(pool).await?;
    }

    for statement in crate::public_dashboard::TABLES {
        sqlx::query(statement).execute(pool).await?;
    }
    for statement in crate::uptime::TABLES {
        sqlx::query(statement).execute(pool).await?;
    }
    for statement in crate::ai::TABLES {
        sqlx::query(statement).execute(pool).await?;
    }
    add_compat_columns(pool).await?;
    migrate_site_memberships(pool).await?;

    for statement in SQLITE_INDEXES {
        sqlx::query(statement).execute(pool).await?;
    }

    Ok(())
}

const CURRENT_SCHEMA_VERSION: i64 = 8;

async fn migrate_site_memberships(pool: &SqlitePool) -> anyhow::Result<()> {
    const MIGRATION_KEY: &str = "migration.site_memberships_v1";

    let mut transaction = pool.begin().await?;
    let already_applied = sqlx::query_scalar::<_, String>(
        "SELECT value_json FROM app_settings WHERE key = ? LIMIT 1",
    )
    .bind(MIGRATION_KEY)
    .fetch_optional(&mut *transaction)
    .await?
    .is_some();
    if already_applied {
        transaction.commit().await?;
        return Ok(());
    }

    // A site's creator remains its owner even when the site predates explicit
    // app membership. This also repairs incomplete legacy ownership rows.
    sqlx::query(
        r#"
        INSERT INTO site_memberships (site_id, user_id, role)
        SELECT sites.id, sites.created_by_user_id, 'owner'
        FROM sites
        WHERE sites.created_by_user_id IS NOT NULL
        ON CONFLICT(site_id, user_id) DO UPDATE SET
            role = 'owner',
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .execute(&mut *transaction)
    .await?;

    // Preserve access to existing apps once. New sites are isolated and gain
    // members only through site_memberships, never implicitly through an org.
    for legacy_membership in [
        r#"
        INSERT INTO site_memberships (site_id, user_id, role)
        SELECT
            sites.id,
            membership.user_id,
            CASE membership.role
                WHEN 'owner' THEN 'owner'
                WHEN 'admin' THEN 'admin'
                ELSE 'read_only'
            END
        FROM sites
        JOIN organization_members AS membership
            ON membership.organization_id = sites.organization_id
        WHERE membership.user_id IS NOT NULL
        ON CONFLICT(site_id, user_id) DO UPDATE SET
            role = CASE
                WHEN site_memberships.role = 'owner' OR excluded.role = 'owner' THEN 'owner'
                WHEN site_memberships.role = 'admin' OR excluded.role = 'admin' THEN 'admin'
                ELSE 'read_only'
            END,
            updated_at = CURRENT_TIMESTAMP
        "#,
        r#"
        INSERT INTO site_memberships (site_id, user_id, role)
        SELECT
            sites.id,
            membership.user_id,
            CASE membership.role
                WHEN 'owner' THEN 'owner'
                WHEN 'admin' THEN 'admin'
                ELSE 'read_only'
            END
        FROM sites
        JOIN team_members AS membership ON membership.team_id = sites.team_id
        WHERE membership.user_id IS NOT NULL
        ON CONFLICT(site_id, user_id) DO UPDATE SET
            role = CASE
                WHEN site_memberships.role = 'owner' OR excluded.role = 'owner' THEN 'owner'
                WHEN site_memberships.role = 'admin' OR excluded.role = 'admin' THEN 'admin'
                ELSE 'read_only'
            END,
            updated_at = CURRENT_TIMESTAMP
        "#,
    ] {
        sqlx::query(legacy_membership)
            .execute(&mut *transaction)
            .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO app_settings (key, value_json, updated_at)
        VALUES (?, 'true', CURRENT_TIMESTAMP)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(MIGRATION_KEY)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(())
}

async fn add_compat_columns(pool: &SqlitePool) -> anyhow::Result<()> {
    migrate_auth_and_onboarding_columns(pool).await?;

    for column in [
        ColumnDef {
            table: "users",
            name: "status",
            ddl: "status TEXT NOT NULL DEFAULT 'active'",
        },
        ColumnDef {
            table: "users",
            name: "last_seen_at",
            ddl: "last_seen_at TEXT",
        },
        ColumnDef {
            table: "auth_login_challenges",
            name: "attempt_count",
            ddl: "attempt_count INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "last_seen_at",
            ddl: "last_seen_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "user_agent_digest",
            ddl: "user_agent_digest TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "step_up_verified_at",
            ddl: "step_up_verified_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "step_up_start_window_at",
            ddl: "step_up_start_window_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "step_up_start_count",
            ddl: "step_up_start_count INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "tracking_rules",
            name: "description",
            ddl: "description TEXT",
        },
        ColumnDef {
            table: "tracking_rules",
            name: "sample_rate",
            ddl: "sample_rate REAL NOT NULL DEFAULT 1.0",
        },
        ColumnDef {
            table: "tracking_rules",
            name: "trigger_config_json",
            ddl: "trigger_config_json TEXT NOT NULL DEFAULT '{}'",
        },
        ColumnDef {
            table: "tracking_rules",
            name: "metadata_json",
            ddl: "metadata_json TEXT NOT NULL DEFAULT '{}'",
        },
        ColumnDef {
            table: "tracking_rules",
            name: "created_by_user_id",
            ddl: "created_by_user_id TEXT",
        },
        ColumnDef {
            table: "tracking_rules",
            name: "updated_at",
            ddl: "updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP",
        },
        ColumnDef {
            table: "users",
            name: "display_name",
            ddl: "display_name TEXT",
        },
        ColumnDef {
            table: "users",
            name: "locale",
            ddl: "locale TEXT NOT NULL DEFAULT 'en'",
        },
        ColumnDef {
            table: "users",
            name: "timezone",
            ddl: "timezone TEXT NOT NULL DEFAULT 'UTC'",
        },
        ColumnDef {
            table: "users",
            name: "tfa_enabled",
            ddl: "tfa_enabled INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "users",
            name: "tfa_secret",
            ddl: "tfa_secret TEXT",
        },
        ColumnDef {
            table: "users",
            name: "tfa_backup_codes",
            ddl: "tfa_backup_codes TEXT",
        },
        ColumnDef {
            table: "users",
            name: "last_login_at",
            ddl: "last_login_at INTEGER",
        },
        ColumnDef {
            table: "users",
            name: "last_login_ip",
            ddl: "last_login_ip TEXT",
        },
        ColumnDef {
            table: "users",
            name: "login_count",
            ddl: "login_count INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "users",
            name: "deleted_at",
            ddl: "deleted_at INTEGER",
        },
        ColumnDef {
            table: "sites",
            name: "team_id",
            ddl: "team_id TEXT",
        },
        ColumnDef {
            table: "sites",
            name: "public_key",
            ddl: "public_key TEXT",
        },
        ColumnDef {
            table: "sites",
            name: "secret_key_hash",
            ddl: "secret_key_hash TEXT",
        },
        ColumnDef {
            table: "sites",
            name: "timezone",
            ddl: "timezone TEXT NOT NULL DEFAULT 'UTC'",
        },
        ColumnDef {
            table: "sites",
            name: "currency",
            ddl: "currency TEXT NOT NULL DEFAULT 'USD'",
        },
        ColumnDef {
            table: "sites",
            name: "public_dashboard",
            ddl: "public_dashboard INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "sites",
            name: "deleted_at",
            ddl: "deleted_at INTEGER",
        },
        ColumnDef {
            table: "sites",
            name: "erasure_pending_at",
            ddl: "erasure_pending_at TEXT",
        },
        ColumnDef {
            table: "site_settings",
            name: "tracking_paused",
            ddl: "tracking_paused INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "site_settings",
            name: "ai_enabled",
            ddl: "ai_enabled INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "site_settings",
            name: "developer_mode",
            ddl: "developer_mode INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "site_settings",
            name: "api_access_enabled",
            ddl: "api_access_enabled INTEGER NOT NULL DEFAULT 0",
        },
        ColumnDef {
            table: "site_settings",
            name: "honor_dnt",
            ddl: "honor_dnt INTEGER NOT NULL DEFAULT 1",
        },
        ColumnDef {
            table: "site_settings",
            name: "data_retention_days",
            ddl: "data_retention_days INTEGER NOT NULL DEFAULT 90",
        },
        ColumnDef {
            table: "site_settings",
            name: "privacy_contact_email",
            ddl: "privacy_contact_email TEXT",
        },
        ColumnDef {
            table: "api_keys",
            name: "team_id",
            ddl: "team_id TEXT",
        },
        ColumnDef {
            table: "api_keys",
            name: "scopes",
            ddl: "scopes TEXT",
        },
    ] {
        add_column_if_missing(pool, column).await?;
    }

    Ok(())
}

async fn migrate_auth_and_onboarding_columns(pool: &SqlitePool) -> anyhow::Result<()> {
    let existing_users_need_backfill =
        !has_column(pool, "users", "onboarding_2fa_handled_at").await?;
    add_column_if_missing(
        pool,
        ColumnDef {
            table: "users",
            name: "onboarding_2fa_handled_at",
            ddl: "onboarding_2fa_handled_at TEXT",
        },
    )
    .await?;
    if existing_users_need_backfill {
        // Existing accounts have already passed the first-run experience. Only users
        // inserted after this migration should receive the optional 2FA prompt.
        sqlx::query(
            "UPDATE users SET onboarding_2fa_handled_at = COALESCE(updated_at, created_at, CURRENT_TIMESTAMP)",
        )
        .execute(pool)
        .await?;
    }

    let legacy_sessions = !has_column(pool, "auth_sessions", "refresh_token_hash").await?;
    for column in [
        ColumnDef {
            table: "auth_sessions",
            name: "access_expires_at",
            ddl: "access_expires_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "previous_access_token_hash",
            ddl: "previous_access_token_hash TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "previous_access_expires_at",
            ddl: "previous_access_expires_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "refresh_token_hash",
            ddl: "refresh_token_hash TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "previous_refresh_token_hash",
            ddl: "previous_refresh_token_hash TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "refresh_expires_at",
            ddl: "refresh_expires_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "refresh_rotated_at",
            ddl: "refresh_rotated_at TEXT",
        },
        ColumnDef {
            table: "auth_sessions",
            name: "refresh_reuse_detected_at",
            ddl: "refresh_reuse_detected_at TEXT",
        },
    ] {
        add_column_if_missing(pool, column).await?;
    }

    if legacy_sessions {
        // A legacy token was valid for the whole session lifetime and has no
        // independently rotatable refresh credential. Force a clean sign-in.
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = COALESCE(revoked_at, CURRENT_TIMESTAMP) WHERE refresh_token_hash IS NULL",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn add_column_if_missing(pool: &SqlitePool, column: ColumnDef<'_>) -> anyhow::Result<()> {
    if has_column(pool, column.table, column.name).await? {
        return Ok(());
    }

    sqlx::query(&format!(
        "ALTER TABLE {} ADD COLUMN {}",
        column.table, column.ddl
    ))
    .execute(pool)
    .await?;

    Ok(())
}

async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> anyhow::Result<bool> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().any(|row| {
        let name: String = row.get("name");
        name == column
    }))
}

struct ColumnDef<'a> {
    table: &'a str,
    name: &'a str,
    ddl: &'a str,
}

const SQLITE_TABLES: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS users (
        id TEXT PRIMARY KEY,
        email TEXT NOT NULL UNIQUE,
        email_verified_at TEXT,
        name TEXT,
        avatar_url TEXT,
        totp_secret TEXT,
        onboarding_2fa_handled_at TEXT,
        status TEXT NOT NULL DEFAULT 'active',
        last_seen_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS organizations (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        slug TEXT NOT NULL UNIQUE,
        created_by_user_id TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        archived_at TEXT,
        FOREIGN KEY(created_by_user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS organization_members (
        organization_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        PRIMARY KEY (organization_id, user_id),
        FOREIGN KEY(organization_id) REFERENCES organizations(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS sites (
        id TEXT PRIMARY KEY,
        organization_id TEXT,
        tracking_id TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        domain TEXT NOT NULL,
        default_timezone TEXT,
        created_by_user_id TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        archived_at TEXT,
        erasure_pending_at TEXT,
        FOREIGN KEY(organization_id) REFERENCES organizations(id) ON DELETE CASCADE,
        FOREIGN KEY(created_by_user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS site_memberships (
        site_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'read_only')),
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        PRIMARY KEY (site_id, user_id),
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS site_domains (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        domain TEXT NOT NULL,
        is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(site_id, domain),
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS site_event_catalog (
        site_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        event_name TEXT NOT NULL,
        first_seen_at TEXT NOT NULL,
        last_seen_at TEXT NOT NULL,
        total_seen INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (site_id, event_type, event_name)
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS tracking_rules (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        selector TEXT NOT NULL,
        type TEXT NOT NULL CHECK (type IN ('click', 'submit', 'view')),
        capture_text INTEGER NOT NULL DEFAULT 0 CHECK (capture_text IN (0, 1)),
        enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
        sample_rate REAL NOT NULL DEFAULT 1.0 CHECK (sample_rate >= 0.0 AND sample_rate <= 1.0),
        trigger_config_json TEXT NOT NULL DEFAULT '{}',
        metadata_json TEXT NOT NULL DEFAULT '{}',
        created_by_user_id TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(created_by_user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS api_keys (
        id TEXT PRIMARY KEY,
        organization_id TEXT,
        site_id TEXT,
        user_id TEXT,
        name TEXT NOT NULL,
        key_prefix TEXT NOT NULL,
        key_hash TEXT NOT NULL UNIQUE,
        scopes_json TEXT NOT NULL DEFAULT '[]',
        last_used_at TEXT,
        expires_at TEXT,
        revoked_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(organization_id) REFERENCES organizations(id) ON DELETE CASCADE,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS app_settings (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS audit_log (
        id TEXT PRIMARY KEY,
        organization_id TEXT,
        user_id TEXT,
        action TEXT NOT NULL,
        target_type TEXT,
        target_id TEXT,
        metadata_json TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(organization_id) REFERENCES organizations(id) ON DELETE SET NULL,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS auth_login_challenges (
        id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        attempt_count INTEGER NOT NULL DEFAULT 0,
        expires_at TEXT NOT NULL,
        verified_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS auth_sessions (
        id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        expires_at TEXT NOT NULL,
        access_expires_at TEXT,
        previous_access_token_hash TEXT,
        previous_access_expires_at TEXT,
        refresh_token_hash TEXT,
        previous_refresh_token_hash TEXT,
        refresh_expires_at TEXT,
        refresh_rotated_at TEXT,
        refresh_reuse_detected_at TEXT,
        last_seen_at TEXT,
        user_agent_digest TEXT,
        step_up_verified_at TEXT,
        step_up_start_window_at TEXT,
        step_up_start_count INTEGER NOT NULL DEFAULT 0,
        revoked_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS auth_step_up_challenges (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        method TEXT NOT NULL CHECK (method IN ('password', 'totp')),
        code_hash TEXT,
        attempt_count INTEGER NOT NULL DEFAULT 0,
        expires_at TEXT NOT NULL,
        consumed_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(session_id) REFERENCES auth_sessions(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    // The following legacy session/team tables (through `team_invites`) are
    // compatibility-reserved. `auth_sessions` and `site_memberships` are the
    // canonical stores. See apps/api/SCHEMA.md before adding runtime usage.
    r#"
    CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        refresh_token_hash TEXT NOT NULL UNIQUE,
        device_name TEXT,
        user_agent TEXT,
        ip_address TEXT,
        country TEXT,
        expires_at INTEGER NOT NULL,
        last_used_at INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS auth_tokens (
        id TEXT PRIMARY KEY,
        user_id TEXT,
        email TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        purpose TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        consumed_at INTEGER,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS teams (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        slug TEXT NOT NULL UNIQUE,
        owner_id TEXT NOT NULL,
        deleted_at INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY(owner_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS team_members (
        team_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
        joined_at INTEGER NOT NULL,
        PRIMARY KEY (team_id, user_id),
        FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS team_invites (
        id TEXT PRIMARY KEY,
        team_id TEXT NOT NULL,
        invited_by TEXT NOT NULL,
        email TEXT NOT NULL,
        role TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        expires_at INTEGER NOT NULL,
        accepted_at INTEGER,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE CASCADE,
        FOREIGN KEY(invited_by) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS site_settings (
        site_id TEXT PRIMARY KEY,
        exclude_internal INTEGER NOT NULL DEFAULT 0,
        honor_dnt INTEGER NOT NULL DEFAULT 1,
        exclude_bots INTEGER NOT NULL DEFAULT 1,
        tracking_paused INTEGER NOT NULL DEFAULT 0,
        ai_enabled INTEGER NOT NULL DEFAULT 0,
        developer_mode INTEGER NOT NULL DEFAULT 0 CHECK (developer_mode IN (0, 1)),
        api_access_enabled INTEGER NOT NULL DEFAULT 0 CHECK (api_access_enabled IN (0, 1)),
        data_retention_days INTEGER NOT NULL DEFAULT 90 CHECK (data_retention_days BETWEEN 1 AND 730),
        privacy_contact_email TEXT,
        hash_salt_rotation TEXT NOT NULL DEFAULT '',
        hash_salt_previous TEXT,
        salt_rotated_at INTEGER NOT NULL DEFAULT 0,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS developer_api_keys (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        created_by_user_id TEXT NOT NULL,
        name TEXT NOT NULL,
        key_prefix TEXT NOT NULL,
        key_hash TEXT NOT NULL UNIQUE,
        scopes_json TEXT NOT NULL,
        last_used_at TEXT,
        expires_at TEXT,
        revoked_at TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(created_by_user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    // `site_exclusions`, `goals`, and the older `rules` table below are
    // compatibility-reserved. Canonical rules live in `tracking_rules`.
    r#"
    CREATE TABLE IF NOT EXISTS site_exclusions (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        exclusion_type TEXT NOT NULL,
        pattern TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS goals (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        name TEXT NOT NULL,
        goal_type TEXT NOT NULL,
        match_rule TEXT NOT NULL,
        value REAL,
        currency TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS rules (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        name TEXT NOT NULL,
        rule_type TEXT NOT NULL,
        selector TEXT,
        match_url_pattern TEXT,
        capture_props TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS dashboards (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        name TEXT NOT NULL,
        layout TEXT,
        is_default INTEGER NOT NULL DEFAULT 0,
        created_by TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(created_by) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS dashboard_charts (
        id TEXT PRIMARY KEY,
        dashboard_id TEXT NOT NULL,
        chart_type TEXT NOT NULL,
        title TEXT NOT NULL,
        query_config TEXT NOT NULL,
        display_config TEXT,
        position_x INTEGER NOT NULL,
        position_y INTEGER NOT NULL,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY(dashboard_id) REFERENCES dashboards(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS campaigns (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        created_by_user_id TEXT NOT NULL,
        name TEXT NOT NULL,
        campaign_key TEXT NOT NULL,
        source TEXT NOT NULL,
        medium TEXT NOT NULL,
        destination_url TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(site_id, campaign_key),
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(created_by_user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS dashboard_shares (
        dashboard_id TEXT NOT NULL,
        site_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        invited_by_user_id TEXT NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'dismissed', 'revoked')),
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        responded_at INTEGER,
        PRIMARY KEY (dashboard_id, user_id),
        FOREIGN KEY(dashboard_id) REFERENCES dashboards(id) ON DELETE CASCADE,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
        FOREIGN KEY(invited_by_user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS segments (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        user_id TEXT NOT NULL,
        name TEXT NOT NULL,
        filters TEXT NOT NULL,
        is_shared INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS webhooks (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        url TEXT NOT NULL,
        secret TEXT NOT NULL,
        events TEXT NOT NULL,
        active INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS webhook_deliveries (
        id TEXT PRIMARY KEY,
        webhook_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        payload TEXT NOT NULL,
        status_code INTEGER,
        response_body TEXT,
        attempt_count INTEGER NOT NULL DEFAULT 0,
        next_retry_at INTEGER,
        delivered_at INTEGER,
        failed_permanently INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(webhook_id) REFERENCES webhooks(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS alerts (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        name TEXT NOT NULL,
        alert_type TEXT NOT NULL,
        metric TEXT NOT NULL,
        condition TEXT NOT NULL,
        notification_channels TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS alert_triggers (
        id TEXT PRIMARY KEY,
        alert_id TEXT NOT NULL,
        triggered_at INTEGER NOT NULL,
        resolved_at INTEGER,
        details TEXT,
        FOREIGN KEY(alert_id) REFERENCES alerts(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS notifications (
        id TEXT PRIMARY KEY,
        user_id TEXT NOT NULL,
        type TEXT NOT NULL,
        title TEXT NOT NULL,
        body TEXT,
        link_url TEXT,
        read_at INTEGER,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS audit_logs (
        id TEXT PRIMARY KEY,
        team_id TEXT,
        user_id TEXT,
        action TEXT NOT NULL,
        resource_type TEXT,
        resource_id TEXT,
        metadata TEXT,
        ip_address TEXT,
        user_agent TEXT,
        created_at INTEGER NOT NULL,
        FOREIGN KEY(team_id) REFERENCES teams(id) ON DELETE SET NULL,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS exports (
        id TEXT PRIMARY KEY,
        site_id TEXT,
        user_id TEXT,
        export_type TEXT NOT NULL,
        format TEXT NOT NULL,
        status TEXT NOT NULL,
        download_url TEXT,
        download_expires_at INTEGER,
        file_size_bytes INTEGER,
        error_message TEXT,
        created_at INTEGER NOT NULL,
        completed_at INTEGER,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE CASCADE,
        FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS retention_jobs (
        id TEXT PRIMARY KEY,
        site_id TEXT,
        site_tracking_id TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('erasure')),
        dedupe_key TEXT NOT NULL UNIQUE,
        priority INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'retry', 'completed', 'cancelled')),
        attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
        next_attempt_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        locked_by TEXT,
        lock_expires_at TEXT,
        mutation_digest TEXT,
        submitted_at TEXT,
        completed_at TEXT,
        last_error TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        FOREIGN KEY(site_id) REFERENCES sites(id) ON DELETE SET NULL
    )
    "#,
];

const SQLITE_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users(email)",
    "CREATE INDEX IF NOT EXISTS idx_organization_members_user ON organization_members(user_id)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_sites_tracking_id ON sites(tracking_id)",
    "CREATE INDEX IF NOT EXISTS idx_sites_organization ON sites(organization_id, archived_at)",
    "CREATE INDEX IF NOT EXISTS idx_site_memberships_user ON site_memberships(user_id, site_id)",
    "CREATE INDEX IF NOT EXISTS idx_site_domains_domain ON site_domains(domain)",
    "CREATE INDEX IF NOT EXISTS idx_site_event_catalog_seen ON site_event_catalog(site_id, last_seen_at)",
    "CREATE INDEX IF NOT EXISTS idx_tracking_rules_site ON tracking_rules(site_id, enabled)",
    "CREATE INDEX IF NOT EXISTS idx_tracking_rules_type ON tracking_rules(site_id, type, enabled)",
    "CREATE INDEX IF NOT EXISTS idx_api_keys_lookup ON api_keys(key_hash, revoked_at)",
    "CREATE INDEX IF NOT EXISTS idx_audit_log_org ON audit_log(organization_id, created_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_login_challenges_user ON auth_login_challenges(user_id, expires_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_login_challenges_cleanup ON auth_login_challenges(expires_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_sessions_token ON auth_sessions(token_hash)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_sessions_refresh ON auth_sessions(refresh_token_hash) WHERE refresh_token_hash IS NOT NULL",
    "CREATE INDEX IF NOT EXISTS idx_auth_sessions_previous_refresh ON auth_sessions(previous_refresh_token_hash) WHERE previous_refresh_token_hash IS NOT NULL",
    "CREATE INDEX IF NOT EXISTS idx_auth_sessions_user ON auth_sessions(user_id, expires_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_sessions_cleanup ON auth_sessions(refresh_expires_at, revoked_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_step_up_session ON auth_step_up_challenges(session_id, expires_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_step_up_cleanup ON auth_step_up_challenges(expires_at, consumed_at)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at)",
    "CREATE INDEX IF NOT EXISTS idx_auth_tokens_hash ON auth_tokens(token_hash)",
    "CREATE INDEX IF NOT EXISTS idx_auth_tokens_expires ON auth_tokens(expires_at)",
    "CREATE INDEX IF NOT EXISTS idx_teams_owner ON teams(owner_id)",
    "CREATE INDEX IF NOT EXISTS idx_teams_slug ON teams(slug)",
    "CREATE INDEX IF NOT EXISTS idx_team_members_user ON team_members(user_id)",
    "CREATE INDEX IF NOT EXISTS idx_invites_team ON team_invites(team_id)",
    "CREATE INDEX IF NOT EXISTS idx_invites_email ON team_invites(email)",
    "CREATE INDEX IF NOT EXISTS idx_invites_token ON team_invites(token_hash)",
    "CREATE INDEX IF NOT EXISTS idx_sites_team ON sites(team_id)",
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_sites_public_key ON sites(public_key) WHERE public_key IS NOT NULL",
    "CREATE INDEX IF NOT EXISTS idx_sites_domain ON sites(domain)",
    "CREATE INDEX IF NOT EXISTS idx_exclusions_site ON site_exclusions(site_id)",
    "CREATE INDEX IF NOT EXISTS idx_goals_site ON goals(site_id)",
    "CREATE INDEX IF NOT EXISTS idx_rules_site ON rules(site_id)",
    "CREATE INDEX IF NOT EXISTS idx_dashboards_site ON dashboards(site_id)",
    "CREATE INDEX IF NOT EXISTS idx_charts_dashboard ON dashboard_charts(dashboard_id)",
    "CREATE INDEX IF NOT EXISTS idx_campaigns_site ON campaigns(site_id, updated_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_dashboard_shares_recipient ON dashboard_shares(site_id, user_id, status, updated_at)",
    "CREATE INDEX IF NOT EXISTS idx_dashboard_shares_dashboard ON dashboard_shares(dashboard_id, status)",
    "CREATE INDEX IF NOT EXISTS idx_developer_api_keys_lookup ON developer_api_keys(key_hash, site_id, revoked_at)",
    "CREATE INDEX IF NOT EXISTS idx_developer_api_keys_site ON developer_api_keys(site_id, created_at)",
    "CREATE INDEX IF NOT EXISTS idx_api_keys_team ON api_keys(team_id)",
    "CREATE INDEX IF NOT EXISTS idx_deliveries_webhook ON webhook_deliveries(webhook_id)",
    "CREATE INDEX IF NOT EXISTS idx_deliveries_retry ON webhook_deliveries(next_retry_at)",
    "CREATE INDEX IF NOT EXISTS idx_notifications_user ON notifications(user_id, created_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_audit_team_time ON audit_logs(team_id, created_at DESC)",
    "CREATE INDEX IF NOT EXISTS idx_retention_jobs_due ON retention_jobs(status, next_attempt_at, lock_expires_at, priority DESC)",
    "CREATE INDEX IF NOT EXISTS idx_retention_jobs_site ON retention_jobs(site_id, status, kind)",
];
