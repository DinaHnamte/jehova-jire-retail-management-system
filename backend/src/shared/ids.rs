use uuid::Uuid;

/// New primary key. UUIDv7 is time-ordered, so B-tree indexes stay compact.
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}
