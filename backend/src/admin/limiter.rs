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
    /// True when another attempt is allowed.
    pub fn check(&self, username: &str, ip: Option<&str>) -> bool {
        let mut map = self.failures.lock().unwrap();
        let now = Instant::now();
        let mut count = |key: String| {
            let v = map.entry(key).or_default();
            v.retain(|t| now.duration_since(*t) < WINDOW);
            v.len()
        };
        count(user_key(username)) < MAX_PER_USERNAME
            && ip.is_none_or(|ip| count(ip_key(ip)) < MAX_PER_IP)
    }

    pub fn record_failure(&self, username: &str, ip: Option<&str>) {
        let mut map = self.failures.lock().unwrap();
        let now = Instant::now();
        map.entry(user_key(username)).or_default().push(now);
        if let Some(ip) = ip {
            map.entry(ip_key(ip)).or_default().push(now);
        }
        // Bound memory: drop keys with no recent failures.
        if map.len() > 10_000 {
            map.retain(|_, v| v.iter().any(|t| now.duration_since(*t) < WINDOW));
        }
    }

    /// Clear the username's failures after a successful login (IP failures stay).
    pub fn reset(&self, username: &str) {
        self.failures.lock().unwrap().remove(&user_key(username));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_five_failures_per_username_case_insensitive() {
        let l = LoginLimiter::default();
        for _ in 0..5 {
            assert!(l.check("Dinah", None));
            l.record_failure("dinah", None);
        }
        assert!(!l.check("DINAH", None));
        assert!(l.check("someone-else", None));
    }

    #[test]
    fn blocks_ip_after_twenty_failures_across_usernames() {
        let l = LoginLimiter::default();
        for i in 0..20 {
            l.record_failure(&format!("user{i}"), Some("1.2.3.4"));
        }
        assert!(!l.check("fresh", Some("1.2.3.4")));
        assert!(l.check("fresh", Some("5.6.7.8")));
    }

    #[test]
    fn reset_clears_username_only() {
        let l = LoginLimiter::default();
        for _ in 0..5 {
            l.record_failure("dinah", None);
        }
        l.reset("dinah");
        assert!(l.check("dinah", None));
    }
}
