//! Evaluates bound expressions with SQL semantics: three-valued logic, NULL
//! propagation, checked integer arithmetic.
//!
//! The binder has already checked Data Types and inserted every Coercion, so
//! both operands of a binary operator always hold Values of the same Data Type (or
//! NULL). Any other combination is a binder bug.

use crate::ast::{BinaryOp, UnaryOp};
use crate::binder::BoundExpr;
use crate::error::{DbError, Result};
use crate::value::{DataType, Value};
use std::cmp::Ordering;

/// Evaluates `expr` against `row`, the Row its Column references read from.
pub fn eval(expr: &BoundExpr, row: &[Value]) -> Result<Value> {
    match expr {
        BoundExpr::Literal(value) => Ok(value.clone()),
        BoundExpr::Column(index) => Ok(row[*index].clone()),
        BoundExpr::ToFloat(operand) => Ok(eval(operand, row)?
            .coerce_to(DataType::Float)
            .expect("binder only wraps INT or NULL in ToFloat")),
        BoundExpr::Unary { op, operand } => unary(*op, eval(operand, row)?),
        BoundExpr::Binary { op, left, right } => binary(*op, eval(left, row)?, eval(right, row)?),
        BoundExpr::IsNull { operand, negated } => {
            let is_null = eval(operand, row)?.is_null();
            Ok(Value::Bool(is_null != *negated))
        }
    }
}

fn unary(op: UnaryOp, value: Value) -> Result<Value> {
    match (op, value) {
        (_, Value::Null) => Ok(Value::Null),
        (UnaryOp::Neg, Value::Int(i)) => i
            .checked_neg()
            .map(Value::Int)
            .ok_or(DbError::IntegerOverflow),
        (UnaryOp::Neg, Value::Float(x)) => Ok(Value::Float(-x)),
        (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (op, value) => unreachable!("binder let through {op} {value:?}"),
    }
}

fn binary(op: BinaryOp, left: Value, right: Value) -> Result<Value> {
    match op {
        // AND and OR can be decided despite a NULL operand, so they come first.
        BinaryOp::And => Ok(and(left.as_bool(), right.as_bool())),
        BinaryOp::Or => Ok(or(left.as_bool(), right.as_bool())),
        _ if left.is_null() || right.is_null() => Ok(Value::Null),
        _ if op.is_comparison() => Ok(Value::Bool(compare(op, &left, &right))),
        BinaryOp::Concat => Ok(concat(left, right)),
        _ => arithmetic(op, left, right),
    }
}

/// Three-valued AND, where `None` is NULL (unknown): FALSE wins over unknown.
fn and(left: Option<bool>, right: Option<bool>) -> Value {
    match (left, right) {
        (Some(false), _) | (_, Some(false)) => Value::Bool(false),
        (Some(true), Some(true)) => Value::Bool(true),
        _ => Value::Null,
    }
}

/// Three-valued OR, where `None` is NULL (unknown): TRUE wins over unknown.
fn or(left: Option<bool>, right: Option<bool>) -> Value {
    match (left, right) {
        (Some(true), _) | (_, Some(true)) => Value::Bool(true),
        (Some(false), Some(false)) => Value::Bool(false),
        _ => Value::Null,
    }
}

/// SQL comparison of two non-NULL Values of the same Data Type.
fn compare(op: BinaryOp, left: &Value, right: &Value) -> bool {
    let ordering = order(left, right);
    match op {
        BinaryOp::Eq => ordering.is_eq(),
        BinaryOp::NotEq => ordering.is_ne(),
        BinaryOp::Lt => ordering.is_lt(),
        BinaryOp::LtEq => ordering.is_le(),
        BinaryOp::Gt => ordering.is_gt(),
        BinaryOp::GtEq => ordering.is_ge(),
        _ => unreachable!("{op} is not a comparison"),
    }
}

/// How two non-NULL Values of one Data Type order. TEXT orders by byte.
pub fn order(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        // Floats are always finite (see float_arithmetic), so never NaN.
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).expect("FLOAT is never NaN"),
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => unreachable!("binder let through {left:?} against {right:?}"),
    }
}

fn concat(left: Value, right: Value) -> Value {
    match (left, right) {
        (Value::Text(a), Value::Text(b)) => Value::Text(a + &b),
        (left, right) => unreachable!("binder let through {left:?} || {right:?}"),
    }
}

fn arithmetic(op: BinaryOp, left: Value, right: Value) -> Result<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => int_arithmetic(op, a, b).map(Value::Int),
        (Value::Float(a), Value::Float(b)) => float_arithmetic(op, a, b).map(Value::Float),
        (left, right) => unreachable!("binder let through {left:?} {op} {right:?}"),
    }
}

fn int_arithmetic(op: BinaryOp, a: i64, b: i64) -> Result<i64> {
    if matches!(op, BinaryOp::Div | BinaryOp::Mod) && b == 0 {
        return Err(DbError::DivisionByZero);
    }
    let result = match op {
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        // Truncates toward zero, like Postgres.
        BinaryOp::Div => a.checked_div(b),
        // i64::MIN % -1 is 0 in SQL, not an overflow.
        BinaryOp::Mod => Some(a.wrapping_rem(b)),
        _ => unreachable!("{op} is not arithmetic"),
    };
    result.ok_or(DbError::IntegerOverflow)
}

fn float_arithmetic(op: BinaryOp, a: f64, b: f64) -> Result<f64> {
    if matches!(op, BinaryOp::Div | BinaryOp::Mod) && b == 0.0 {
        return Err(DbError::DivisionByZero);
    }
    let result = match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        BinaryOp::Mod => a % b,
        _ => unreachable!("{op} is not arithmetic"),
    };
    // Literals are finite and every operation is checked, so inf and NaN
    // never reach a Value.
    if result.is_finite() {
        Ok(result)
    } else {
        Err(DbError::FloatOverflow)
    }
}
