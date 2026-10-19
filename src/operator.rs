//! The Operators a Plan is built from (ADR-0002). Each one yields Rows on
//! demand, pulling from its children.

use crate::binder::BoundExpr;
use crate::error::Result;
use crate::eval::eval;
use crate::table::{Row, RowId, Table};
use crate::value::Value;

/// A Row as it flows through a Plan.
///
/// It carries the Row ID of the Table Row it came from, so a Plan can tell
/// UPDATE and DELETE which Rows matched. Operators that keep a Row's identity
/// (Filter, Project, and later Sort or Limit) pass the Row ID through unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRow {
    /// `None` for a Row that comes from no Table, such as SingleRow's.
    pub row_id: Option<RowId>,
    pub values: Vec<Value>,
}

pub trait Operator {
    /// The next Row, or `None` once the Operator is exhausted.
    fn next(&mut self) -> Result<Option<PlanRow>>;
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
    fn next(&mut self) -> Result<Option<PlanRow>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        Ok(Some(PlanRow {
            row_id: None,
            values: Vec::new(),
        }))
    }
}

/// Yields every Row of a Table, with its Row ID, in Row ID order.
pub struct Scan<'a> {
    rows: Box<dyn Iterator<Item = (RowId, &'a Row)> + 'a>,
}

impl<'a> Scan<'a> {
    pub fn new(table: &'a Table) -> Self {
        Scan {
            rows: Box::new(table.iter_with_ids()),
        }
    }
}

impl Operator for Scan<'_> {
    fn next(&mut self) -> Result<Option<PlanRow>> {
        Ok(self.rows.next().map(|(id, row)| PlanRow {
            row_id: Some(id),
            values: row.values().to_vec(),
        }))
    }
}

/// Passes on only the Rows of its child for which the predicate is TRUE.
/// FALSE and NULL (unknown) both drop the Row.
pub struct Filter<'a> {
    child: Box<dyn Operator + 'a>,
    predicate: BoundExpr,
}

impl<'a> Filter<'a> {
    pub fn new(child: Box<dyn Operator + 'a>, predicate: BoundExpr) -> Self {
        Filter { child, predicate }
    }
}

impl Operator for Filter<'_> {
    fn next(&mut self) -> Result<Option<PlanRow>> {
        while let Some(row) = self.child.next()? {
            if eval(&self.predicate, &row.values)? == Value::Bool(true) {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
}

/// Computes one output Value per expression for each Row of its child.
pub struct Project<'a> {
    child: Box<dyn Operator + 'a>,
    exprs: Vec<BoundExpr>,
}

impl<'a> Project<'a> {
    pub fn new(child: Box<dyn Operator + 'a>, exprs: Vec<BoundExpr>) -> Self {
        Project { child, exprs }
    }
}

impl Operator for Project<'_> {
    fn next(&mut self) -> Result<Option<PlanRow>> {
        let Some(row) = self.child.next()? else {
            return Ok(None);
        };
        let values = self
            .exprs
            .iter()
            .map(|expr| eval(expr, &row.values))
            .collect::<Result<Vec<Value>>>()?;
        Ok(Some(PlanRow {
            row_id: row.row_id,
            values,
        }))
    }
}
