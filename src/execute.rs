//! The library's entry point for SQL: one Statement in, an Output or an error out.

use crate::binder::{BoundSelect, BoundStatement, bind};
use crate::database::Database;
use crate::error::Result;
use crate::parser::parse;
use crate::planner::plan_select;
use crate::value::{DataType, Value};

/// What a successful Statement produces.
#[derive(Debug, Clone)]
pub enum Output {
    /// The Statement ran and returns no Rows (e.g. CREATE TABLE, INSERT).
    Done {
        /// psql-style command tag, e.g. "CREATE TABLE", "INSERT 0 2", "UPDATE 3".
        tag: String,
        /// Rows inserted, updated or deleted; 0 for Statements that change no Rows.
        rows_affected: u64,
    },
    /// The Statement returns Rows (e.g. SELECT).
    Rows(ResultSet),
}

/// Rows returned by a Statement, with the names and Data Types of their Columns.
///
/// Rows are plain `Vec<Value>` rather than `table::Row`, because a result Row
/// (e.g. a projection or an aggregate) need not belong to any Table.
#[derive(Debug, Clone)]
pub struct ResultSet {
    pub columns: Vec<String>,
    /// One per Column, in the same order as `columns`.
    pub types: Vec<DataType>,
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

/// Runs exactly one SQL Statement against the Database.
///
/// The Statement may end with a `;`. Anything after that first `;` (such as a
/// second Statement) is an error, not silently ignored; the parser enforces this.
/// Splitting several Statements typed on one line is the caller's job.
pub fn execute(db: &mut Database, sql: &str) -> Result<Output> {
    let statement = parse(sql)?;
    match bind(db, statement)? {
        BoundStatement::Select(select) => run_select(select).map(Output::Rows),
    }
}

fn run_select(select: BoundSelect) -> Result<ResultSet> {
    let (columns, types) = select
        .columns
        .iter()
        .map(|column| (column.name.clone(), column.data_type))
        .unzip();
    let mut plan = plan_select(select);
    let mut rows = Vec::new();
    while let Some(row) = plan.next()? {
        rows.push(row);
    }
    Ok(ResultSet {
        columns,
        types,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DbError;

    fn query(sql: &str) -> ResultSet {
        match execute(&mut Database::new(), sql).unwrap() {
            Output::Rows(result) => result,
            other => panic!("expected Rows, got {other:?}"),
        }
    }

    fn error(sql: &str) -> String {
        execute(&mut Database::new(), sql).unwrap_err().to_string()
    }

    #[test]
    fn as_names_an_output_column() {
        let result = query("SELECT 1 AS one, 2");
        assert_eq!(result.columns, vec!["one", "?column?"]);
    }

    #[test]
    fn a_second_statement_is_an_error() {
        assert_eq!(
            error("SELECT 1; SELECT 2;"),
            "line 1, col 11: expected end of statement"
        );
    }

    #[test]
    fn a_trailing_semicolon_is_optional() {
        assert_eq!(query("SELECT 1;").rows, query("SELECT 1").rows);
        assert!(matches!(
            execute(&mut Database::new(), "SELECT 1;;"),
            Err(DbError::Syntax { .. })
        ));
    }

    #[test]
    fn errors_at_end_of_input_point_past_the_last_character() {
        assert_eq!(error("SELECT"), "line 1, col 7: expected an expression");
        assert_eq!(
            error("SELECT 1,\n  "),
            "line 2, col 3: expected an expression"
        );
    }

    #[test]
    fn lexer_errors_surface_as_syntax_errors() {
        assert_eq!(error("SELECT \"x\""), "line 1, col 8: double-quoted identifiers are not supported");
    }
}
