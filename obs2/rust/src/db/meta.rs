use anyhow::Context;
use rusqlite::{Connection, OptionalExtension};

pub const SCHEMA_VERSION: i64 = 3;

// Use WAL so reads can continue while catalog writes are committed.
const PRAGMA_JOURNAL_MODE_WAL: &str = "PRAGMA journal_mode = WAL";
// Enforce relational constraints for future catalog tables that may reference each other.
const PRAGMA_FOREIGN_KEYS_ON: &str = "PRAGMA foreign_keys = ON";
// Store catalog/schema metadata such as schema_version.
const CREATE_META_TABLE: &str = "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)";
// Record the schema version this binary knows how to read/write.
const SET_SCHEMA_VERSION: &str = "INSERT OR REPLACE INTO meta(key, value) VALUES ('schema_version', ?1)";
// Read the schema version so supported upgrades can migrate explicitly.
const GET_SCHEMA_VERSION: &str = "SELECT value FROM meta WHERE key = 'schema_version'";
// Drop the meta table when reseeding the documented schema-1 catalog.
const DROP_META_TABLE: &str = "DROP TABLE IF EXISTS meta";

pub fn initialise(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(PRAGMA_JOURNAL_MODE_WAL)?;
    conn.execute_batch(PRAGMA_FOREIGN_KEYS_ON)?;
    conn.execute_batch(CREATE_META_TABLE)?;
    Ok(())
}

pub fn set_schema_version(conn: &Connection) -> anyhow::Result<()> {
    conn.execute(SET_SCHEMA_VERSION, [SCHEMA_VERSION.to_string()])?;
    Ok(())
}

pub fn drop_tables(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(DROP_META_TABLE)?;
    Ok(())
}

pub fn stored_schema_version(conn: &Connection) -> anyhow::Result<Option<i64>> {
    let value: Option<String> = conn.query_row(GET_SCHEMA_VERSION, [], |row| row.get(0)).optional()?;
    value
        .map(|value| value.parse().with_context(|| format!("invalid run catalog schema version {value:?}")))
        .transpose()
}
