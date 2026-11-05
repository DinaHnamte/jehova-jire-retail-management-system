use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub database_url: String,
    pub database_max_connections: u32,
    /// Adds `Secure` to the session cookie. Only disable for plain-HTTP local development.
    pub cookie_secure: bool,
    /// Trust `X-Forwarded-For` for the client IP (true only behind Caddy).
    pub trust_proxy: bool,
    /// `json` (production) or `pretty`.
    pub log_format: String,
    /// Apply pending migrations at startup (development convenience).
    pub run_migrations: bool,
    /// Public base URL of the app, no trailing slash; receipt links are built on it.
    pub public_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            bind: var_or("APP_BIND", "127.0.0.1:8000"),
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?,
            database_max_connections: var_or("DATABASE_MAX_CONNECTIONS", "10")
                .parse()
                .context("DATABASE_MAX_CONNECTIONS must be a number")?,
            cookie_secure: parse_bool("COOKIE_SECURE", true)?,
            trust_proxy: parse_bool("TRUST_PROXY", false)?,
            log_format: var_or("LOG_FORMAT", "json"),
            run_migrations: parse_bool("RUN_MIGRATIONS", false)?,
            public_url: normalize_public_url(std::env::var("PUBLIC_URL").ok()),
        })
    }
}

pub const DEFAULT_PUBLIC_URL: &str = "https://jehovah-jire.hnamtek.in";

/// Unset or blank means the production default; a trailing slash is dropped.
fn normalize_public_url(raw: Option<String>) -> String {
    let v = raw.unwrap_or_default();
    let v = v.trim().trim_end_matches('/');
    if v.is_empty() { DEFAULT_PUBLIC_URL } else { v }.to_string()
}

fn var_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_bool(key: &str, default: bool) -> Result<bool> {
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => anyhow::bail!("{key} must be true or false"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_url_defaults_and_trims() {
        assert_eq!(normalize_public_url(None), DEFAULT_PUBLIC_URL);
        assert_eq!(normalize_public_url(Some("  ".into())), DEFAULT_PUBLIC_URL);
        assert_eq!(
            normalize_public_url(Some("https://x.test/".into())),
            "https://x.test"
        );
    }
}
