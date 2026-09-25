use ipnet::IpNet;
use std::{env, net::SocketAddr, path::PathBuf};
#[derive(Clone)]
pub struct Settings {
    pub ai: crate::ai::AiSettings,
    pub allow_loopback_origins: bool,
    pub api_addr: SocketAddr,
    pub auth: AuthSettings,
    pub clickhouse_url: String,
    pub hash_salt: String,
    pub maxmind_db: Option<PathBuf>,
    pub sqlite_url: String,
    pub trusted_proxies: Vec<IpNet>,
}
#[derive(Clone)]
pub struct AuthSettings {
    pub access_minutes: i64,
    pub allowed_origins: Vec<String>,
    pub app_url: String,
    pub cookie_name: String,
    pub cookie_secure: bool,
    pub refresh_cookie_name: String,
    pub refresh_days: i64,
}
impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            access_minutes: 60,
            allowed_origins: vec!["http://localhost:8527".into()],
            app_url: "http://localhost:8527".into(),
            cookie_name: "owleye_session".into(),
            cookie_secure: false,
            refresh_cookie_name: "owleye_refresh".into(),
            refresh_days: 30,
        }
    }
}
impl AuthSettings {
    pub(crate) fn console_origins(&self) -> impl Iterator<Item = &str> {
        self.allowed_origins.iter().map(String::as_str)
    }
}
impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        let app_url = env::var("OWLEYE_APP_URL").unwrap_or_else(|_| "http://localhost:8527".into());
        let url = reqwest::Url::parse(&app_url)?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/",
            "OWLEYE_APP_URL must be an HTTP(S) origin without a path or credentials"
        );
        let app_url = url.origin().ascii_serialization();
        let hash_salt = env::var("OWLEYE_HASH_SALT")?;
        anyhow::ensure!(
            hash_salt.len() >= 32,
            "OWLEYE_HASH_SALT must contain at least 32 random characters"
        );
        Ok(Self {
            ai: crate::ai::AiSettings::from_env()?,
            allow_loopback_origins: env::var("OWLEYE_ALLOW_LOOPBACK_ORIGINS")
                .is_ok_and(|v| v == "true"),
            api_addr: env::var("OWLEYE_API_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:8527".into())
                .parse()?,
            auth: AuthSettings {
                allowed_origins: vec![app_url.clone()],
                app_url,
                cookie_secure: url.scheme() == "https",
                ..Default::default()
            },
            clickhouse_url: env::var("CLICKHOUSE_URL")?,
            sqlite_url: env::var("SQLITE_URL")?,
            hash_salt,
            maxmind_db: env::var("OWLEYE_MAXMIND_DB")
                .ok()
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            trusted_proxies: env::var("OWLEYE_TRUSTED_PROXIES")
                .unwrap_or_default()
                .split(',')
                .filter(|v| !v.trim().is_empty())
                .map(|v| v.trim().parse())
                .collect::<Result<_, _>>()?,
        })
    }
}
