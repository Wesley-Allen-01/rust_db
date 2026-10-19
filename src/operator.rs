//! The Operators a Plan is built from (ADR-0002). Each one yields Rows on
//! demand, pulling from its children. Rows are plain `Vec<Value>`, as in a
//! Result Set, because they need not belong to any Table.

use crate::binder::BoundExpr;
use crate::error::Result;
use crate::eval::eval;
use crate::value::Value;

pub trait Operator {
    /// The next Row, or `None` once the Operator is exhausted.
    fn next(&mut self) -> Result<Option<Vec<Value>>>;
}

/// Yields one empty Row, then stops. The source for a SELECT without FROM.
#[derive(Debug, Default)]
pub struct SingleRow {
    done: bool,
}

impl SingleRow {
    pub fn new() -> Self {
        SingleRow::default()
    }
}

impl Operator for SingleRow {
    fn next(&mut self) -> Result<Option<Vec<Value>>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        Ok(Some(Vec::new()))
    }
}

/// Computes one output Value per expression for each Row of its child.
pub struct Project {
    child: Box<dyn Operator>,
    exprs: Vec<BoundExpr>,
}

impl Project {
    pub fn new(child: Box<dyn Operator>, exprs: Vec<BoundExpr>) -> Self {
        Project { child, exprs }
    }
}

impl Operator for Project {
    fn next(&mut self) -> Result<Option<Vec<Value>>> {
        // No expression reads the child's Row until Column references can be
        // bound (they need FROM); the Row only drives how often we emit.
        if self.child.next()?.is_none() {
            return Ok(None);
        }
        let projected = self
            .exprs
            .iter()
            .map(eval)
            .collect::<Result<Vec<Value>>>()?;
        Ok(Some(projected))
    }
}
