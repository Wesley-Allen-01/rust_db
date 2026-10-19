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
        Ok(self.rows.next().map(|(row_id, row)| PlanRow {
            row_id: Some(row_id),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinaryOp;
    use crate::schema::{Column, Schema};
    use crate::value::DataType;

    fn drain(mut op: impl Operator) -> Vec<PlanRow> {
        let mut rows = Vec::new();
        while let Some(row) = op.next().unwrap() {
            rows.push(row);
        }
        rows
    }

    fn row(row_id: Option<RowId>, values: Vec<Value>) -> PlanRow {
        PlanRow { row_id, values }
    }

    /// A child Operator that yields the given Rows.
    struct Rows(std::vec::IntoIter<PlanRow>);

    impl Operator for Rows {
        fn next(&mut self) -> Result<Option<PlanRow>> {
            Ok(self.0.next())
        }
    }

    fn child(rows: Vec<PlanRow>) -> Box<dyn Operator> {
        Box::new(Rows(rows.into_iter()))
    }

    fn binary(op: BinaryOp, left: BoundExpr, right: BoundExpr) -> BoundExpr {
        BoundExpr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    #[test]
    fn single_row_yields_one_empty_row_without_a_row_id() {
        assert_eq!(drain(SingleRow::new()), vec![row(None, vec![])]);
    }

    #[test]
    fn scan_yields_each_row_with_its_row_id_skipping_deleted_ones() {
        let schema = Schema::new(vec![Column::new("n", DataType::Int)]).unwrap();
        let mut table = Table::new("t", schema);
        for n in [10, 20, 30] {
            table.insert(vec![Value::Int(n)]).unwrap();
        }
        table.delete(1);
        assert_eq!(
            drain(Scan::new(&table)),
            vec![
                row(Some(0), vec![Value::Int(10)]),
                row(Some(2), vec![Value::Int(30)]),
            ]
        );
    }

    #[test]
    fn filter_keeps_only_rows_where_the_predicate_is_true() {
        // n > 1 is FALSE for 1, NULL (unknown) for NULL, and TRUE for 2.
        let predicate = binary(
            BinaryOp::Gt,
            BoundExpr::Column(0),
            BoundExpr::Literal(Value::Int(1)),
        );
        let rows = child(vec![
            row(Some(5), vec![Value::Int(1)]),
            row(Some(6), vec![Value::Null]),
            row(Some(7), vec![Value::Int(2)]),
        ]);
        assert_eq!(
            drain(Filter::new(rows, predicate)),
            vec![row(Some(7), vec![Value::Int(2)])]
        );
    }

    #[test]
    fn project_evaluates_each_expression_against_the_row_and_keeps_its_row_id() {
        let exprs = vec![
            BoundExpr::Column(1),
            binary(
                BinaryOp::Mul,
                BoundExpr::Column(0),
                BoundExpr::Literal(Value::Int(10)),
            ),
        ];
        let rows = child(vec![row(
            Some(3),
            vec![Value::Int(4), Value::Text("a".into())],
        )]);
        assert_eq!(
            drain(Project::new(rows, exprs)),
            vec![row(Some(3), vec![Value::Text("a".into()), Value::Int(40)])]
        );
    }
}
