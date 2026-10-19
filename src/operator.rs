//! The Operators a Plan is built from (ADR-0002). Each one yields Rows on
//! demand, pulling from its children.

use crate::binder::BoundExpr;
use crate::error::Result;
use crate::eval::eval;
use crate::value::Value;

/// A Row flowing between Operators. A plain `Vec<Value>` rather than
/// `table::Row`, because these Rows need not belong to any Table.
pub type Row = Vec<Value>;

pub trait Operator {
    /// The next Row, or `None` once the Operator is exhausted.
    fn next(&mut self) -> Result<Option<Row>>;
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
    fn next(&mut self) -> Result<Option<Row>> {
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
    fn next(&mut self) -> Result<Option<Row>> {
        let Some(row) = self.child.next()? else {
            return Ok(None);
        };
        let projected = self
            .exprs
            .iter()
            .map(|expr| eval(expr, &row))
            .collect::<Result<Row>>()?;
        Ok(Some(projected))
    }
}
