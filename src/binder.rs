//! Checks a parsed Statement's names and Data Types before anything runs (ADR-0003).
//!
//! The output is a bound Statement: every expression is known to be well-typed,
//! and every INT-to-FLOAT Coercion within an expression is an explicit `ToFloat`, so
//! evaluation never has to look at types. A Value going into a Column is coerced by
//! the Table instead, which does it for every Row it stores.

use crate::ast::{
    BinaryOp, CreateTable, Delete, DropTable, Expr, Insert, OrderByItem, Select, SelectItem,
    Statement, UnaryOp, Update,
};
use crate::database::Database;
use crate::error::{DbError, Result};
use crate::eval::eval;
use crate::schema::{Column, ResultColumn, Schema};
use crate::table::Table;
use crate::value::{DataType, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum BoundStatement {
    Select(BoundSelect),
    CreateTable(BoundCreateTable),
    DropTable(BoundDropTable),
    Insert(BoundInsert),
    Update(BoundUpdate),
    Delete(BoundDelete),
    /// A Statement that `IF [NOT] EXISTS` turned into nothing. It still
    /// reports its command tag, as in Postgres.
    NoOp {
        tag: &'static str,
    },
}

/// A CREATE TABLE whose name is free and whose Columns form a valid Schema.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundCreateTable {
    pub name: String,
    pub schema: Schema,
}

/// A DROP TABLE of a Table that exists.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundDropTable {
    pub name: String,
}

/// An INSERT into a Table that exists. Each row holds one expression per
/// Column, in Schema order, with NULL for Columns the column list left out.
/// Each expression's Data Type fits its Column, allowing for the Coercion
/// the Table applies.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundInsert {
    pub table: String,
    pub rows: Vec<Vec<BoundExpr>>,
}

/// A SELECT whose names all resolve. Every expression, WHERE included, reads
/// the Rows of the `from` Table, or a single empty Row without FROM.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundSelect {
    /// The Table's name as stored, whatever case the query used.
    pub from: Option<String>,
    /// The WHERE, known to be BOOL (or NULL).
    pub predicate: Option<BoundExpr>,
    /// The SELECT list, with `*` expanded.
    pub exprs: Vec<BoundExpr>,
    /// One per expression, in the same order.
    pub columns: Vec<ResultColumn>,
    /// The ORDER BY keys, most significant first. Each reads the same Rows
    /// as the SELECT list, not its output.
    pub order_by: Vec<SortKey>,
    /// The most Rows to return, if there is a LIMIT.
    pub limit: Option<usize>,
    /// How many Rows to skip first: 0 without OFFSET.
    pub offset: usize,
}

/// One ORDER BY key. NULLs sort last for ASC and first for DESC, as in Postgres.
#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub expr: BoundExpr,
    pub descending: bool,
}

/// An UPDATE of a Table that exists. Every expression, WHERE included, reads
/// the Row as it was before the UPDATE.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundUpdate {
    /// The Table's name as stored, whatever case the query used.
    pub table: String,
    /// Each target Column's index with its new value, whose Data Type fits the
    /// Column, allowing for the Coercion the Table applies. Each Column
    /// appears at most once.
    pub assignments: Vec<(usize, BoundExpr)>,
    /// The WHERE, known to be BOOL (or NULL).
    pub predicate: Option<BoundExpr>,
}

/// A DELETE from a Table that exists.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundDelete {
    /// The Table's name as stored, whatever case the query used.
    pub table: String,
    /// The WHERE, over the Table's Rows, known to be BOOL (or NULL).
    pub predicate: Option<BoundExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundExpr {
    Literal(Value),
    /// The Value at this index in the Row being evaluated, in Schema order.
    Column(usize),
    Unary {
        op: UnaryOp,
        operand: Box<BoundExpr>,
    },
    /// Both operands share one Data Type (or are NULL); Coercion has already
    /// been applied.
    Binary {
        op: BinaryOp,
        left: Box<BoundExpr>,
        right: Box<BoundExpr>,
    },
    /// `IS NULL`, or `IS NOT NULL` when `negated`.
    IsNull {
        operand: Box<BoundExpr>,
        negated: bool,
    },
    /// The INT-to-FLOAT Coercion.
    ToFloat(Box<BoundExpr>),
}

/// An expression's Data Type, or `None` while it is only known to be NULL.
type ExprType = Option<DataType>;

pub fn bind(db: &Database, statement: Statement) -> Result<BoundStatement> {
    match statement {
        Statement::Select(select) => bind_select(db, select),
        Statement::CreateTable(create) => bind_create_table(db, create),
        Statement::DropTable(drop) => bind_drop_table(db, drop),
        Statement::Insert(insert) => bind_insert(db, insert),
        Statement::Update(update) => bind_update(db, update),
        Statement::Delete(delete) => bind_delete(db, delete),
    }
}

fn bind_insert(db: &Database, insert: Insert) -> Result<BoundStatement> {
    let table = db.get(&insert.table)?;
    let schema = table.schema();
    let targets = match insert.columns {
        Some(names) => resolve_targets(schema, &names)?,
        None => (0..schema.len()).collect(),
    };
    let rows = insert
        .rows
        .into_iter()
        .map(|row| bind_insert_row(schema, &targets, row))
        .collect::<Result<_>>()?;
    Ok(BoundStatement::Insert(BoundInsert {
        table: table.name().to_string(),
        rows,
    }))
}

/// The index of each Column in an INSERT's column list. A name may appear
/// only once.
fn resolve_targets(schema: &Schema, names: &[String]) -> Result<Vec<usize>> {
    let mut targets = Vec::new();
    for name in names {
        let index = schema.resolve(name)?;
        if targets.contains(&index) {
            return Err(DbError::DuplicateColumn { name: name.clone() });
        }
        targets.push(index);
    }
    Ok(targets)
}

/// Binds one VALUES row, whose values go to the Columns at `targets`, into a
/// full-width row in Schema order. Every other Column gets NULL.
fn bind_insert_row(schema: &Schema, targets: &[usize], row: Vec<Expr>) -> Result<Vec<BoundExpr>> {
    if row.len() != targets.len() {
        return Err(DbError::ArityMismatch {
            expected: targets.len(),
            got: row.len(),
        });
    }
    let mut bound = vec![BoundExpr::Literal(Value::Null); schema.len()];
    for (expr, &index) in row.into_iter().zip(targets) {
        bound[index] = bind_column_value(&schema.columns()[index], expr, None)?;
    }
    Ok(bound)
}

/// Binds a value for `column`, read over the Columns of `scope`, checking its
/// Data Type. NOT NULL is left to the Table, since an expression such as
/// `NULL + 1` is only known to be NULL once evaluated.
fn bind_column_value(column: &Column, expr: Expr, scope: Option<&Schema>) -> Result<BoundExpr> {
    let (expr, ty) = bind_expr(expr, scope)?;
    match ty {
        Some(ty) if !ty.coerces_to(column.data_type()) => Err(DbError::TypeMismatch {
            column: column.name().to_string(),
            expected: column.data_type(),
            got: Some(ty),
        }),
        _ => Ok(expr),
    }
}

fn bind_update(db: &Database, update: Update) -> Result<BoundStatement> {
    let table = db.get(&update.table)?;
    let schema = table.schema();
    let names: Vec<String> = update
        .assignments
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    let targets = resolve_targets(schema, &names)?;
    let assignments = update
        .assignments
        .into_iter()
        .zip(targets)
        .map(|((_, expr), index)| {
            let value = bind_column_value(&schema.columns()[index], expr, Some(schema))?;
            Ok((index, value))
        })
        .collect::<Result<_>>()?;
    Ok(BoundStatement::Update(BoundUpdate {
        table: table.name().to_string(),
        assignments,
        predicate: bind_predicate(update.predicate, Some(schema))?,
    }))
}

fn bind_delete(db: &Database, delete: Delete) -> Result<BoundStatement> {
    let table = db.get(&delete.table)?;
    Ok(BoundStatement::Delete(BoundDelete {
        table: table.name().to_string(),
        predicate: bind_predicate(delete.predicate, Some(table.schema()))?,
    }))
}

/// The name is checked before the Schema, so `IF NOT EXISTS` on an existing
/// Table skips the Statement without looking at its Columns, as in Postgres.
fn bind_create_table(db: &Database, create: CreateTable) -> Result<BoundStatement> {
    if db.contains_table(&create.name) {
        if create.if_not_exists {
            return Ok(BoundStatement::NoOp {
                tag: "CREATE TABLE",
            });
        }
        return Err(DbError::TableAlreadyExists { name: create.name });
    }
    let columns = create
        .columns
        .into_iter()
        .map(|def| {
            let column = Column::new(def.name, def.data_type);
            if def.not_null {
                column.not_null()
            } else {
                column
            }
        })
        .collect();
    Ok(BoundStatement::CreateTable(BoundCreateTable {
        name: create.name,
        schema: Schema::new(columns)?,
    }))
}

fn bind_drop_table(db: &Database, drop: DropTable) -> Result<BoundStatement> {
    // `get` gives the same NoSuchTable error that INSERT and SELECT will.
    match db.get(&drop.name) {
        Ok(table) => Ok(BoundStatement::DropTable(BoundDropTable {
            name: table.name().to_string(),
        })),
        Err(DbError::NoSuchTable { .. }) if drop.if_exists => {
            Ok(BoundStatement::NoOp { tag: "DROP TABLE" })
        }
        Err(err) => Err(err),
    }
}

/// The Table named in FROM, if any, is the scope for every expression,
/// including WHERE.
fn bind_select(db: &Database, select: Select) -> Result<BoundStatement> {
    let table = select
        .from
        .as_deref()
        .map(|name| db.get(name))
        .transpose()?;
    let scope = table.map(Table::schema);
    let mut exprs = Vec::new();
    let mut columns = Vec::new();
    for item in select.items {
        match item {
            SelectItem::Wildcard => {
                let (wildcard_exprs, wildcard_columns) = bind_wildcard(scope)?;
                exprs.extend(wildcard_exprs);
                columns.extend(wildcard_columns);
            }
            SelectItem::Expr { expr, alias } => {
                let (expr, column) = bind_select_expr(expr, alias, scope)?;
                exprs.push(expr);
                columns.push(column);
            }
        }
    }
    let order_by = select
        .order_by
        .into_iter()
        .map(|item| bind_sort_key(item, &exprs, &columns, scope))
        .collect::<Result<_>>()?;
    Ok(BoundStatement::Select(BoundSelect {
        from: table.map(|table| table.name().to_string()),
        predicate: bind_predicate(select.predicate, scope)?,
        exprs,
        columns,
        order_by,
        limit: bind_count("LIMIT", select.limit)?,
        offset: bind_count("OFFSET", select.offset)?.unwrap_or(0),
    }))
}

/// Binds the count of a LIMIT or OFFSET `clause`: a constant INT that is not
/// negative. Being constant, it is evaluated here, once.
fn bind_count(clause: &'static str, count: Option<Expr>) -> Result<Option<usize>> {
    let Some(count) = count else {
        return Ok(None);
    };
    // No scope: a constant can't read a Column.
    let (count, ty) = bind_expr(count, None)?;
    if ty != Some(DataType::Int) {
        return Err(DbError::CountNotInt { clause, got: ty });
    }
    match eval(&count, &[])? {
        Value::Int(n) if n < 0 => Err(DbError::NegativeCount { clause }),
        // An i64 that is not negative always fits a usize on 64-bit targets.
        Value::Int(n) => Ok(Some(usize::try_from(n).unwrap_or(usize::MAX))),
        // An INT expression such as `NULL + 1` can still evaluate to NULL.
        _ => Err(DbError::CountNotInt { clause, got: None }),
    }
}

/// As in Postgres, a bare INT is a position in the SELECT list, and a bare
/// name that is a Result Set Column's name sorts by that Column's expression, so
/// an alias works as a key. Any other key is an expression over the input
/// Columns.
fn bind_sort_key(
    item: OrderByItem,
    exprs: &[BoundExpr],
    columns: &[ResultColumn],
    scope: Option<&Schema>,
) -> Result<SortKey> {
    let expr = match &item.expr {
        Expr::Literal(Value::Int(position)) => select_list_at(*position, exprs)?,
        Expr::Column(name) => match select_list_expr(name, exprs, columns)? {
            Some(expr) => expr,
            None => bind_expr(item.expr, scope)?.0,
        },
        _ => bind_expr(item.expr, scope)?.0,
    };
    Ok(SortKey {
        expr,
        descending: item.descending,
    })
}

/// The expression of the Result Set Column at `position`, counting from 1.
fn select_list_at(position: i64, exprs: &[BoundExpr]) -> Result<BoundExpr> {
    usize::try_from(position)
        .ok()
        .and_then(|position| exprs.get(position.checked_sub(1)?))
        .cloned()
        .ok_or(DbError::NoSuchPosition { position })
}

/// The expression of the Result Set Column called `name`, if there is one.
/// Several such Columns are fine only if they compute the same thing.
fn select_list_expr(
    name: &str,
    exprs: &[BoundExpr],
    columns: &[ResultColumn],
) -> Result<Option<BoundExpr>> {
    let mut matches = columns
        .iter()
        .zip(exprs)
        .filter(|(column, _)| column.name.eq_ignore_ascii_case(name))
        .map(|(_, expr)| expr);
    let Some(first) = matches.next() else {
        return Ok(None);
    };
    if matches.any(|other| other != first) {
        return Err(DbError::AmbiguousColumn {
            name: name.to_string(),
        });
    }
    Ok(Some(first.clone()))
}

/// `*` expands to every Column of the FROM Table, in Schema order.
fn bind_wildcard(scope: Option<&Schema>) -> Result<(Vec<BoundExpr>, Vec<ResultColumn>)> {
    let schema = scope.ok_or(DbError::WildcardWithoutTable)?;
    let exprs = (0..schema.len()).map(BoundExpr::Column).collect();
    let columns = schema
        .columns()
        .iter()
        .map(|column| ResultColumn::new(column.name(), column.data_type()))
        .collect();
    Ok((exprs, columns))
}

/// Binds a WHERE, which must be BOOL. NULL is allowed: it drops every Row.
fn bind_predicate(predicate: Option<Expr>, scope: Option<&Schema>) -> Result<Option<BoundExpr>> {
    let Some(predicate) = predicate else {
        return Ok(None);
    };
    match bind_expr(predicate, scope)? {
        (_, Some(ty)) if ty != DataType::Bool => Err(DbError::WhereNotBool { got: ty }),
        (predicate, _) => Ok(Some(predicate)),
    }
}

/// The Result Set Column is named by the alias, or else by the expression's
/// text, which for a Column reference is the Column's name.
fn bind_select_expr(
    expr: Expr,
    alias: Option<String>,
    scope: Option<&Schema>,
) -> Result<(BoundExpr, ResultColumn)> {
    let name = alias.unwrap_or_else(|| expr.to_string());
    let (expr, ty) = bind_expr(expr, scope)?;
    // A Column that is only ever NULL gets TEXT, as Postgres does for its
    // "unknown" type.
    let data_type = ty.unwrap_or(DataType::Text);
    Ok((expr, ResultColumn::new(name, data_type)))
}

fn bind_expr(expr: Expr, scope: Option<&Schema>) -> Result<(BoundExpr, ExprType)> {
    match expr {
        Expr::Literal(value) => {
            let ty = value.type_of();
            Ok((BoundExpr::Literal(value), ty))
        }
        Expr::Column(name) => {
            // Without FROM there are no Columns in scope.
            let schema = scope.ok_or_else(|| DbError::NoSuchColumn { name: name.clone() })?;
            let index = schema.resolve(&name)?;
            let ty = schema.columns()[index].data_type();
            Ok((BoundExpr::Column(index), Some(ty)))
        }
        Expr::Unary { op, operand } => bind_unary(op, *operand, scope),
        Expr::Binary { op, left, right } => bind_binary(op, *left, *right, scope),
        Expr::IsNull { operand, negated } => {
            // Any Data Type can be tested for NULL.
            let (operand, _) = bind_expr(*operand, scope)?;
            let bound = BoundExpr::IsNull {
                operand: Box::new(operand),
                negated,
            };
            Ok((bound, Some(DataType::Bool)))
        }
    }
}

fn bind_unary(op: UnaryOp, operand: Expr, scope: Option<&Schema>) -> Result<(BoundExpr, ExprType)> {
    let (operand, ty) = bind_expr(operand, scope)?;
    let result_ty = match op {
        UnaryOp::Neg if is_numeric(ty) => numeric_result(ty),
        UnaryOp::Not if fits(ty, DataType::Bool) => Some(DataType::Bool),
        _ => return Err(DbError::UnaryTypeMismatch { op, operand: ty }),
    };
    let bound = BoundExpr::Unary {
        op,
        operand: Box::new(operand),
    };
    Ok((bound, result_ty))
}

fn bind_binary(
    op: BinaryOp,
    left: Expr,
    right: Expr,
    scope: Option<&Schema>,
) -> Result<(BoundExpr, ExprType)> {
    let (left, left_ty) = bind_expr(left, scope)?;
    let (right, right_ty) = bind_expr(right, scope)?;
    let mismatch = DbError::BinaryTypeMismatch {
        op,
        left: left_ty,
        right: right_ty,
    };
    let both = |data_type| fits(left_ty, data_type) && fits(right_ty, data_type);

    let (left, right, ty) = match op {
        BinaryOp::And | BinaryOp::Or if both(DataType::Bool) => (left, right, Some(DataType::Bool)),
        BinaryOp::Concat if both(DataType::Text) => (left, right, Some(DataType::Text)),
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if is_numeric(left_ty) && is_numeric(right_ty) =>
        {
            let (left, right, ty) = unify((left, left_ty), (right, right_ty)).ok_or(mismatch)?;
            (left, right, numeric_result(ty))
        }
        op if op.is_comparison() => {
            let (left, right, _) = unify((left, left_ty), (right, right_ty)).ok_or(mismatch)?;
            (left, right, Some(DataType::Bool))
        }
        _ => return Err(mismatch),
    };
    let bound = BoundExpr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    };
    Ok((bound, ty))
}

/// Brings two operands to one Data Type, applying INT-to-FLOAT Coercion if
/// needed. `None` if their Data Types can't be reconciled.
fn unify(
    (left, left_ty): (BoundExpr, ExprType),
    (right, right_ty): (BoundExpr, ExprType),
) -> Option<(BoundExpr, BoundExpr, ExprType)> {
    match (left_ty, right_ty) {
        (Some(l), Some(r)) if l == r => Some((left, right, left_ty)),
        (Some(l), Some(r)) if l.coerces_to(r) => Some((to_float(left), right, right_ty)),
        (Some(l), Some(r)) if r.coerces_to(l) => Some((left, to_float(right), left_ty)),
        (Some(_), Some(_)) => None,
        // NULL takes on the other operand's Data Type.
        _ => Some((left, right, left_ty.or(right_ty))),
    }
}

fn to_float(expr: BoundExpr) -> BoundExpr {
    BoundExpr::ToFloat(Box::new(expr))
}

/// Whether an operand of type `ty` can stand where `expected` is needed.
/// NULL can stand anywhere.
fn fits(ty: ExprType, expected: DataType) -> bool {
    ty.is_none_or(|t| t == expected)
}

/// Arithmetic on only NULLs is INT, so its result can't then pass as BOOL or
/// TEXT (ADR-0003).
fn numeric_result(ty: ExprType) -> ExprType {
    ty.or(Some(DataType::Int))
}

fn is_numeric(ty: ExprType) -> bool {
    matches!(ty, None | Some(DataType::Int) | Some(DataType::Float))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn bind_sql(db: &Database, sql: &str) -> Result<BoundStatement> {
        bind(db, parse(sql).unwrap())
    }

    fn db_with_users() -> Database {
        let mut db = Database::new();
        let schema = Schema::new(vec![Column::new("id", DataType::Int)]).unwrap();
        db.create_table("users", schema).unwrap();
        db
    }

    // Here rather than in slt, which can't tell the binder's errors from the
    // Database's.
    #[test]
    fn binder_rejects_an_existing_table_name() {
        assert_eq!(
            bind_sql(&db_with_users(), "CREATE TABLE users (id INT)"),
            Err(DbError::TableAlreadyExists {
                name: "users".to_string()
            })
        );
    }

    #[test]
    fn binder_rejects_dropping_an_unknown_table() {
        assert_eq!(
            bind_sql(&Database::new(), "DROP TABLE users"),
            Err(DbError::NoSuchTable {
                name: "users".to_string()
            })
        );
    }

    #[test]
    fn if_exists_variants_bind_to_a_no_op_with_their_tag() {
        assert_eq!(
            bind_sql(
                &db_with_users(),
                "CREATE TABLE IF NOT EXISTS users (id INT)"
            ),
            Ok(BoundStatement::NoOp {
                tag: "CREATE TABLE"
            })
        );
        assert_eq!(
            bind_sql(&Database::new(), "DROP TABLE IF EXISTS users"),
            Ok(BoundStatement::NoOp { tag: "DROP TABLE" })
        );
    }
}
