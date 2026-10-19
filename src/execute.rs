//! The library's entry point for SQL: one Statement in, an Output or an error out.

use crate::binder::{BoundDelete, BoundInsert, BoundSelect, BoundStatement, BoundUpdate, bind};
use crate::database::Database;
use crate::error::Result;
use crate::eval::eval;
use crate::operator::PlanRow;
use crate::parser::parse;
use crate::planner::{plan_matching_rows, plan_select};
use crate::schema::ResultColumn;
use crate::table::RowId;
use crate::value::Value;

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
    pub columns: Vec<ResultColumn>,
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
/// For now that means a REPL line holding several Statements is rejected too.
pub fn execute(db: &mut Database, sql: &str) -> Result<Output> {
    let statement = parse(sql)?;
    match bind(db, statement)? {
        BoundStatement::Select(select) => run_select(db, select).map(Output::Rows),
        // The binder has checked the name; the Database checks it again as a guard.
        BoundStatement::CreateTable(create) => {
            db.create_table(create.name, create.schema)?;
            Ok(done("CREATE TABLE"))
        }
        BoundStatement::DropTable(drop) => {
            db.drop_table(&drop.name)?;
            Ok(done("DROP TABLE"))
        }
        BoundStatement::Insert(insert) => run_insert(db, insert),
        BoundStatement::Update(update) => run_update(db, update),
        BoundStatement::Delete(delete) => run_delete(db, delete),
        BoundStatement::NoOp { tag } => Ok(done(tag)),
    }
}

/// Evaluates every row before inserting any, and the Table inserts them all
/// or none, so a failing row leaves the Table unchanged.
fn run_insert(db: &mut Database, insert: BoundInsert) -> Result<Output> {
    let rows = insert
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|expr| eval(expr, &[]))
                .collect::<Result<Vec<Value>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let count = rows.len() as u64;
    db.get_mut(&insert.table)?.insert_all(rows)?;
    Ok(Output::Done {
        // The 0 is the OID psql reports; it is always 0 in modern Postgres.
        tag: format!("INSERT 0 {count}"),
        rows_affected: count,
    })
}

/// Works out every changed Row before updating any, and the Table updates
/// them all or none, so a failing Row leaves the Table unchanged.
fn run_update(db: &mut Database, update: BoundUpdate) -> Result<Output> {
    let rows = {
        let mut plan = plan_matching_rows(db.get(&update.table)?, update.predicate);
        let mut rows = Vec::new();
        while let Some(row) = plan.next()? {
            // Every SET reads the old Values, so evaluate all before setting any.
            let mut values = row.values.clone();
            for (index, expr) in &update.assignments {
                values[*index] = eval(expr, &row.values)?;
            }
            rows.push((row_id(&row), values));
        }
        rows
    };
    let count = db.get_mut(&update.table)?.update_all(rows)?;
    Ok(changed("UPDATE", count))
}

/// Finds every matching Row before deleting any, so the Table isn't changed
/// while it is being scanned.
fn run_delete(db: &mut Database, delete: BoundDelete) -> Result<Output> {
    let ids = {
        let mut plan = plan_matching_rows(db.get(&delete.table)?, delete.predicate);
        let mut ids: Vec<RowId> = Vec::new();
        while let Some(row) = plan.next()? {
            ids.push(row_id(&row));
        }
        ids
    };
    let table = db.get_mut(&delete.table)?;
    for &id in &ids {
        table.delete(id);
    }
    Ok(changed("DELETE", ids.len()))
}

fn row_id(row: &PlanRow) -> RowId {
    row.row_id.expect("a Scanned Row has a Row ID")
}

/// The Output of a Statement that changed `count` Rows, e.g. "DELETE 3".
fn changed(command: &str, count: usize) -> Output {
    Output::Done {
        tag: format!("{command} {count}"),
        rows_affected: count as u64,
    }
}

/// The Output of a Statement that changes no Rows.
fn done(tag: &str) -> Output {
    Output::Done {
        tag: tag.to_string(),
        rows_affected: 0,
    }
}

fn run_select(db: &Database, select: BoundSelect) -> Result<ResultSet> {
    let columns = select.columns.clone();
    let mut plan = plan_select(db, select)?;
    let mut rows = Vec::new();
    while let Some(row) = plan.next()? {
        rows.push(row.values);
    }
    Ok(ResultSet { columns, rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::DataType;

    fn tag(db: &mut Database, sql: &str) -> String {
        match execute(db, sql).unwrap() {
            Output::Done { tag, .. } => tag,
            other => panic!("expected Done, got {other:?}"),
        }
    }

    // Here rather than in slt, which discards the tag.
    #[test]
    fn create_and_drop_report_psql_status_tags() {
        let mut db = Database::new();
        assert_eq!(tag(&mut db, "CREATE TABLE t (id INT)"), "CREATE TABLE");
        assert_eq!(tag(&mut db, "DROP TABLE t"), "DROP TABLE");
    }

    #[test]
    fn insert_reports_a_psql_status_tag_and_row_count() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT)").unwrap();
        let output = execute(&mut db, "INSERT INTO t VALUES (1), (2)").unwrap();
        let Output::Done { tag, rows_affected } = output else {
            panic!("expected Done");
        };
        assert_eq!(tag, "INSERT 0 2");
        assert_eq!(rows_affected, 2);
    }

    #[test]
    fn update_and_delete_report_psql_status_tags() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT)").unwrap();
        execute(&mut db, "INSERT INTO t VALUES (1), (2), (3)").unwrap();
        assert_eq!(tag(&mut db, "UPDATE t SET id = 0 WHERE id > 1"), "UPDATE 2");
        assert_eq!(tag(&mut db, "DELETE FROM t"), "DELETE 3");
    }

    // Here rather than in slt, which can't check Column names.
    fn result_columns(db: &mut Database, sql: &str) -> Vec<ResultColumn> {
        match execute(db, sql).unwrap() {
            Output::Rows(result) => result.columns,
            other => panic!("expected Rows, got {other:?}"),
        }
    }

    #[test]
    fn a_result_column_is_named_by_alias_then_column_name_then_expression_text() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT, name TEXT)").unwrap();
        let sql = "SELECT id + 1 AS next, Name, -id * (2 + 1.5), 'it''s', name IS NOT NULL FROM t";
        assert_eq!(
            result_columns(&mut db, sql),
            vec![
                ResultColumn::new("next", DataType::Int),
                // The lexer folds names to lowercase.
                ResultColumn::new("name", DataType::Text),
                ResultColumn::new("(-id) * (2 + 1.5)", DataType::Float),
                ResultColumn::new("'it''s'", DataType::Text),
                ResultColumn::new("name IS NOT NULL", DataType::Bool),
            ]
        );
    }

    #[test]
    fn star_names_each_column_as_the_schema_does() {
        let mut db = Database::new();
        execute(&mut db, "CREATE TABLE t (id INT, score FLOAT)").unwrap();
        assert_eq!(
            result_columns(&mut db, "SELECT * FROM t"),
            vec![
                ResultColumn::new("id", DataType::Int),
                ResultColumn::new("score", DataType::Float),
            ]
        );
    }
}
