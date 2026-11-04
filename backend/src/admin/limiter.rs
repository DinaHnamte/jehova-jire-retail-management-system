//! In-memory login throttling: 5 failures per username and 20 per IP in 15 minutes.
//! Single-server deployment (ADR-0002), so process memory is enough; restarts reset it.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub const WINDOW: Duration = Duration::from_secs(15 * 60);
pub const MAX_PER_USERNAME: usize = 5;
pub const MAX_PER_IP: usize = 20;

#[derive(Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, Vec<Instant>>>,
}

fn user_key(username: &str) -> String {
    format!("u:{}", username.trim().to_lowercase())
}
fn ip_key(ip: &str) -> String {
    format!("ip:{ip}")
}

impl LoginLimiter {
    /// Reserve one login attempt. Returns false (and reserves nothing) when the username
    /// or IP has used up its attempts. Checking and reserving happen under one lock, so
    /// parallel requests can't all slip through before the first failure is recorded.
    /// A reserved attempt counts as a failure unless [`succeeded`](Self::succeeded) is called.
    pub fn try_acquire(&self, username: &str, ip: Option<&str>) -> bool {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let mut recent = |key: &str| {
            let v = map.entry(key.to_string()).or_default();
            v.retain(|t| now.duration_since(*t) < WINDOW);
            v.len()
        };
        let ukey = user_key(username);
        let ikey = ip.map(ip_key);
        let allowed = recent(&ukey) < MAX_PER_USERNAME
            && ikey.as_deref().is_none_or(|k| recent(k) < MAX_PER_IP);
        if allowed {
            map.entry(ukey).or_default().push(now);
            if let Some(k) = ikey {
                map.entry(k).or_default().push(now);
            }
        }
        // Bound memory: drop keys with no recent attempts.
        if map.len() > 10_000 {
            map.retain(|_, v| v.iter().any(|t| now.duration_since(*t) < WINDOW));
        }
        allowed
    }

    /// The reserved attempt succeeded: clear the username's failures and give back
    /// the IP's reservation (earlier IP failures stay).
    pub fn succeeded(&self, username: &str, ip: Option<&str>) {
        let mut map = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&user_key(username));
        if let Some(v) = ip.and_then(|ip| map.get_mut(&ip_key(ip))) {
            v.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_five_failures_per_username_case_insensitive() {
        let l = LoginLimiter::default();
        for _ in 0..5 {
            assert!(l.try_acquire("Dinah", None));
        }
        assert!(!l.try_acquire("DINAH", None));
        assert!(l.try_acquire("someone-else", None));
    }

    #[test]
    fn blocks_ip_after_twenty_failures_across_usernames() {
        let l = LoginLimiter::default();
        for i in 0..20 {
            assert!(l.try_acquire(&format!("user{i}"), Some("1.2.3.4")));
        }
        assert!(!l.try_acquire("fresh", Some("1.2.3.4")));
        assert!(l.try_acquire("fresh", Some("5.6.7.8")));
    }

    #[test]
    fn reset_clears_username_only() {
        let l = LoginLimiter::default();
        for _ in 0..5 {
            l.try_acquire("dinah", None);
        }
        l.succeeded("dinah", None);
        assert!(l.try_acquire("dinah", None));
    }
}

#[cfg(test)]
mod reserve_tests {
    use super::*;

    #[test]
    fn try_acquire_counts_before_the_outcome_is_known() {
        let l = LoginLimiter::default();
        for _ in 0..5 {
            assert!(l.try_acquire("dinah", None));
        }
        assert!(!l.try_acquire("dinah", None));
    }

    #[test]
    fn success_releases_the_reservation() {
        let l = LoginLimiter::default();
        for _ in 0..4 {
            assert!(l.try_acquire("dinah", Some("1.1.1.1")));
        }
        l.succeeded("dinah", Some("1.1.1.1"));
        for _ in 0..5 {
            assert!(l.try_acquire("dinah", None));
        }
    }
}
