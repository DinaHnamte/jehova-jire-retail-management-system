//! In-memory sliding-window request limiter, keyed by a string (a client IP).
//! Single-server deployment (ADR-0002), so process memory is enough; restarts reset it.
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct RateLimiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Counts one request for `key`. Returns false (and counts nothing) when `key` already
    /// made `max` requests within the window.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        // Drop idle keys now and then so the map can't grow without bound.
        if hits.len() > 10_000 {
            hits.retain(|_, q| {
                q.back()
                    .is_some_and(|t| now.duration_since(*t) < self.window)
            });
        }
        let q = hits.entry(key.to_string()).or_default();
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            q.pop_front();
        }
        if q.len() >= self.max {
            return false;
        }
        q.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_key_and_recovers() {
        let l = RateLimiter::new(2, Duration::from_millis(50));
        assert!(l.check("a"));
        assert!(l.check("a"));
        assert!(!l.check("a"));
        assert!(l.check("b"));
        std::thread::sleep(Duration::from_millis(60));
        assert!(l.check("a"));
    }
}
