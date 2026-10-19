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
