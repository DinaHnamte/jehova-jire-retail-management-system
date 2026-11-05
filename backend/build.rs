// Rebuild when a migration is added or changed: `sqlx::migrate!()` embeds the
// migrations at compile time, and cargo doesn't otherwise watch that folder.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
