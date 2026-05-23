use rusqlite_migration::{Migrations, M};

const INITIAL: &str = include_str!("../../migrations/0001_initial.sql");

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(INITIAL)])
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
