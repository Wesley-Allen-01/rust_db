//! Turns a bound Statement into a Plan: a tree of Operators (ADR-0002).

use crate::binder::{BoundExpr, BoundSelect};
use crate::database::Database;
use crate::error::Result;
use crate::operator::{Filter, Operator, Project, Scan, SingleRow};
use crate::table::Table;

/// Builds `Project(Filter?(Scan | SingleRow))`. Operators that work on whole
/// input Rows, such as Sort, go between the Filter and the Project.
pub fn plan_select<'a>(db: &'a Database, select: BoundSelect) -> Result<Box<dyn Operator + 'a>> {
    let rows = match &select.from {
        // The binder has checked the name; `get` checks it again as a guard.
        Some(name) => plan_matching_rows(db.get(name)?, select.predicate),
        // Without FROM, the SELECT list is computed once, over a single empty Row.
        None => with_filter(Box::new(SingleRow::new()), select.predicate),
    };
    Ok(Box::new(Project::new(rows, select.exprs)))
}

/// The Rows of `table` for which `predicate` is TRUE (every Row without one),
/// in Row ID order, each with its Row ID.
pub fn plan_matching_rows<'a>(
    table: &'a Table,
    predicate: Option<BoundExpr>,
) -> Box<dyn Operator + 'a> {
    with_filter(Box::new(Scan::new(table)), predicate)
}

fn with_filter<'a>(
    child: Box<dyn Operator + 'a>,
    predicate: Option<BoundExpr>,
) -> Box<dyn Operator + 'a> {
    match predicate {
        Some(predicate) => Box::new(Filter::new(child, predicate)),
        None => child,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::UnaryOp;
    use crate::schema::{Column, Schema};
    use crate::table::RowId;
    use crate::value::{DataType, Value};

    fn matching_row_ids(table: &Table, predicate: Option<BoundExpr>) -> Vec<RowId> {
        let mut plan = plan_matching_rows(table, predicate);
        let mut ids = Vec::new();
        while let Some(row) = plan.next().unwrap() {
            ids.push(row.row_id.expect("a Scanned Row has a Row ID"));
        }
        ids
    }

    #[test]
    fn matching_rows_carry_the_row_ids_update_and_delete_act_on() {
        let schema = Schema::new(vec![Column::new("on", DataType::Bool)]).unwrap();
        let mut table = Table::new("t", schema);
        for value in [
            Value::Bool(true),
            Value::Bool(false),
            Value::Null,
            Value::Bool(true),
        ] {
            table.insert(vec![value]).unwrap();
        }
        assert_eq!(matching_row_ids(&table, None), vec![0, 1, 2, 3]);
        assert_eq!(
            matching_row_ids(&table, Some(BoundExpr::Column(0))),
            vec![0, 3]
        );
        // NOT NULL is still NULL, so Row 2 matches neither way.
        let negated = BoundExpr::Unary {
            op: UnaryOp::Not,
            operand: Box::new(BoundExpr::Column(0)),
        };
        assert_eq!(matching_row_ids(&table, Some(negated)), vec![1]);
    }
}
