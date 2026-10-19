//! The library's entry point for SQL: one Statement in, an Output or an error out.

use crate::database::Database;
use crate::error::{DbError, Result};
use crate::value::Value;

/// What a successful Statement produces.
#[derive(Debug, Clone)]
pub enum Output {
    /// The Statement ran and returns no Rows (e.g. CREATE TABLE).
    Done,
    /// The Statement returns Rows (e.g. SELECT).
    Rows(ResultSet),
}

/// Rows returned by a Statement, with the names of their Columns.
///
/// Rows are plain `Vec<Value>` rather than `table::Row`, because a result Row
/// (e.g. a projection or an aggregate) need not belong to any Table.
#[derive(Debug, Clone)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl ResultSet {
    /// Each Row's Values as display text.
    pub fn text_rows(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|row| row.iter().map(|value| value.to_string()).collect())
            .collect()
    }
}

/// Runs one SQL Statement against the Database.
pub fn execute(_db: &mut Database, _sql: &str) -> Result<Output> {
    // Placeholder until the lexer and parser exist.
    Err(DbError::NotYetSupported)
}
