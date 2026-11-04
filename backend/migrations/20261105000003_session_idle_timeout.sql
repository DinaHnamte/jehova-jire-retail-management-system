-- Idle timeout per session: 12 h for staff, 30 days for driver-only accounts (Architecture.md §13).
ALTER TABLE sessions ADD COLUMN idle_timeout_secs INTEGER NOT NULL DEFAULT 43200
    CHECK (idle_timeout_secs > 0);
