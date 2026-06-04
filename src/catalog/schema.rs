use rusqlite_migration::{Migrations, M};

const INITIAL: &str = include_str!("../../migrations/0001_initial.sql");
const METADATA: &str = include_str!("../../migrations/0002_metadata.sql");
const SETTINGS: &str = include_str!("../../migrations/0003_settings.sql");
const EMBED_STATE: &str = include_str!("../../migrations/0004_embed_state.sql");
const CONTENT_HASH: &str = include_str!("../../migrations/0005_content_hash.sql");
const READING_PROGRESS: &str = include_str!("../../migrations/0006_reading_progress.sql");
const DEVICES: &str = include_str!("../../migrations/0007_devices.sql");

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(INITIAL),
        M::up(METADATA),
        M::up(SETTINGS),
        M::up(EMBED_STATE),
        M::up(CONTENT_HASH),
        M::up(READING_PROGRESS),
        M::up(DEVICES),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migrations_apply_cleanly_on_fresh_db() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut conn).unwrap();
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations().to_latest(&mut conn).unwrap();
        migrations().to_latest(&mut conn).unwrap();
    }
}
