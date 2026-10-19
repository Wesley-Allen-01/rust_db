//! Evaluates bound expressions against a Row, with SQL semantics: three-valued
//! logic, NULL propagation, checked integer arithmetic.
//!
//! The binder has already checked Data Types and inserted every Coercion, so
//! both operands of a binary operator always hold the same kind of Value (or
//! NULL). Any other combination is a binder bug.

use crate::ast::{BinaryOp, UnaryOp};
use crate::binder::BoundExpr;
use crate::error::{DbError, Result};
use crate::value::Value;

pub fn eval(expr: &BoundExpr, row: &[Value]) -> Result<Value> {
    match expr {
        BoundExpr::Literal(value) => Ok(value.clone()),
        BoundExpr::ToFloat(operand) => Ok(match eval(operand, row)? {
            Value::Int(i) => Value::Float(i as f64),
            other => other,
        }),
        BoundExpr::Unary { op, operand } => unary(*op, eval(operand, row)?),
        BoundExpr::Binary { op, left, right } => {
            binary(*op, eval(left, row)?, eval(right, row)?)
        }
    }
}

fn unary(op: UnaryOp, value: Value) -> Result<Value> {
    match (op, value) {
        (_, Value::Null) => Ok(Value::Null),
        (UnaryOp::Neg, Value::Int(i)) => i.checked_neg().map(Value::Int).ok_or(DbError::IntegerOverflow),
        (UnaryOp::Neg, Value::Float(x)) => Ok(Value::Float(-x)),
        (op, value) => unreachable!("binder let through {op} {value:?}"),
    }
}

fn binary(op: BinaryOp, left: Value, right: Value) -> Result<Value> {
    if left.is_null() || right.is_null() {
        return Ok(Value::Null);
    }
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
    Ok(match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        BinaryOp::Mod => a % b,
        _ => unreachable!("{op} is not arithmetic"),
    })
}
