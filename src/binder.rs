//! Checks a parsed Statement's names and Data Types before anything runs (ADR-0003).
//!
//! The output is a bound Statement: every expression is known to be well-typed,
//! and every INT-to-FLOAT Coercion is an explicit node, so evaluation never has
//! to look at types.

use crate::ast::{Expr, SelectItem, Statement};
use crate::database::Database;
use crate::error::{DbError, Result};
use crate::value::{DataType, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum BoundStatement {
    Select(BoundSelect),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoundSelect {
    pub exprs: Vec<BoundExpr>,
    pub columns: Vec<OutputColumn>,
}

/// A Result Set Column: its name and Data Type.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputColumn {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BoundExpr {
    Literal(Value),
}

/// An expression's Data Type, or `None` while it is only known to be NULL.
type ExprType = Option<DataType>;

pub fn bind(_db: &Database, statement: Statement) -> Result<BoundStatement> {
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
    }
}

fn bind_select_item(item: SelectItem) -> Result<(BoundExpr, OutputColumn)> {
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
    Ok((expr, OutputColumn { name, data_type }))
}

fn bind_expr(expr: Expr) -> Result<(BoundExpr, ExprType)> {
    match expr {
        Expr::Literal(value) => {
            let ty = value.type_of();
            Ok((BoundExpr::Literal(value), ty))
        }
        Expr::Column(name) => Err(DbError::NoSuchColumn { name }),
        _ => Err(DbError::NotYetSupported),
    }
}
