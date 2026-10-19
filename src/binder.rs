//! Checks a parsed Statement's names and Data Types before anything runs (ADR-0003).
//!
//! The output is a bound Statement: every expression is known to be well-typed,
//! and every INT-to-FLOAT Coercion within an expression is an explicit `ToFloat`, so
//! evaluation never has to look at types. A Value going into a Column is coerced by
//! the Table instead, which does it for every Row it stores.

use crate::ast::{BinaryOp, CreateTable, DropTable, Expr, Insert, SelectItem, Statement, UnaryOp};
use crate::database::Database;
use crate::error::{DbError, Result};
use crate::schema::{Column, ResultColumn, Schema};
use crate::value::{DataType, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum BoundStatement {
    Select(BoundSelect),
    CreateTable(BoundCreateTable),
    DropTable(BoundDropTable),
    Insert(BoundInsert),
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

#[derive(Debug, Clone, PartialEq)]
pub struct BoundSelect {
    pub exprs: Vec<BoundExpr>,
    /// One per expression, in the same order.
    pub columns: Vec<ResultColumn>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundExpr {
    Literal(Value),
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
        Statement::Select(select) => {
            let mut exprs = Vec::new();
            let mut columns = Vec::new();
            for item in select.items {
                let (expr, column) = bind_select_item(item)?;
                exprs.push(expr);
                columns.push(column);
            }
            Ok(BoundStatement::Select(BoundSelect { exprs, columns }))
        }
        Statement::CreateTable(create) => bind_create_table(db, create),
        Statement::DropTable(drop) => bind_drop_table(db, drop),
        Statement::Insert(insert) => bind_insert(db, insert),
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
        bound[index] = bind_insert_value(&schema.columns()[index], expr)?;
    }
    Ok(bound)
}

/// Binds a value for `column`, checking its Data Type. NOT NULL is left to
/// the Table, since an expression such as `NULL + 1` is only known to be NULL
/// once evaluated.
fn bind_insert_value(column: &Column, expr: Expr) -> Result<BoundExpr> {
    let (expr, ty) = bind_expr(expr)?;
    match ty {
        Some(ty) if !ty.coerces_to(column.data_type()) => Err(DbError::TypeMismatch {
            column: column.name().to_string(),
            expected: column.data_type(),
            got: Some(ty),
        }),
        _ => Ok(expr),
    }
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

fn bind_select_item(item: SelectItem) -> Result<(BoundExpr, ResultColumn)> {
    let name = match (&item.alias, &item.expr) {
        (Some(alias), _) => alias.clone(),
        (None, Expr::Column(name)) => name.clone(),
        // Postgres's name for an unnamed expression.
        (None, _) => "?column?".to_string(),
    };
    let (expr, ty) = bind_expr(item.expr)?;
    // A Column that is only ever NULL gets TEXT, as Postgres does for its
    // "unknown" type.
    let data_type = ty.unwrap_or(DataType::Text);
    Ok((expr, ResultColumn::new(name, data_type)))
}

fn bind_expr(expr: Expr) -> Result<(BoundExpr, ExprType)> {
    match expr {
        Expr::Literal(value) => {
            let ty = value.type_of();
            Ok((BoundExpr::Literal(value), ty))
        }
        // Without FROM there are no Columns in scope.
        Expr::Column(name) => Err(DbError::NoSuchColumn { name }),
        Expr::Unary { op, operand } => bind_unary(op, *operand),
        Expr::Binary { op, left, right } => bind_binary(op, *left, *right),
        Expr::IsNull { operand, negated } => {
            // Any Data Type can be tested for NULL.
            let (operand, _) = bind_expr(*operand)?;
            let bound = BoundExpr::IsNull {
                operand: Box::new(operand),
                negated,
            };
            Ok((bound, Some(DataType::Bool)))
        }
    }
}

fn bind_unary(op: UnaryOp, operand: Expr) -> Result<(BoundExpr, ExprType)> {
    let (operand, ty) = bind_expr(operand)?;
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

fn bind_binary(op: BinaryOp, left: Expr, right: Expr) -> Result<(BoundExpr, ExprType)> {
    let (left, left_ty) = bind_expr(left)?;
    let (right, right_ty) = bind_expr(right)?;
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
    use DataType::{Float, Int};
    match (left_ty, right_ty) {
        (Some(Int), Some(Float)) => Some((to_float(left), right, Some(Float))),
        (Some(Float), Some(Int)) => Some((left, to_float(right), Some(Float))),
        // NULL takes on the other operand's Data Type.
        (Some(l), Some(r)) if l != r => None,
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
