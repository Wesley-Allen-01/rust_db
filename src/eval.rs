//! Evaluates bound expressions against a Row, with SQL semantics: three-valued
//! logic, NULL propagation, checked integer arithmetic.
//!
//! The binder has already checked types, so a mismatch here is a bug.

use crate::binder::BoundExpr;
use crate::error::Result;
use crate::value::Value;

pub fn eval(expr: &BoundExpr, _row: &[Value]) -> Result<Value> {
    match expr {
        BoundExpr::Literal(value) => Ok(value.clone()),
    }
}
