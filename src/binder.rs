//! Checks a parsed Statement's names and Data Types before anything runs (ADR-0003).
//!
//! The output is a bound Statement: every expression is known to be well-typed,
//! and every INT-to-FLOAT Coercion is an explicit `ToFloat`, so evaluation never has
//! to look at types.

use crate::ast::{BinaryOp, Expr, SelectItem, Statement, UnaryOp};
use crate::database::Database;
use crate::error::{DbError, Result};
use crate::execute::ResultColumn;
use crate::value::{DataType, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum BoundStatement {
    Select(BoundSelect),
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
        UnaryOp::Neg if is_numeric(ty) => ty,
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
            unify((left, left_ty), (right, right_ty)).ok_or(mismatch)?
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

fn is_numeric(ty: ExprType) -> bool {
    matches!(ty, None | Some(DataType::Int) | Some(DataType::Float))
}
