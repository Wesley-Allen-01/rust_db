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
        Some(name) => plan_matching_rows(db.get(name)?, select.filter),
        // Without FROM, the SELECT list is computed once, over a single empty Row.
        None => filtered(Box::new(SingleRow::new()), select.filter),
    };
    Ok(Box::new(Project::new(rows, select.exprs)))
}

/// The Rows of `table` for which `filter` is TRUE (every Row without one),
/// in Row ID order, each with its Row ID.
pub fn plan_matching_rows<'a>(
    table: &'a Table,
    filter: Option<BoundExpr>,
) -> Box<dyn Operator + 'a> {
    filtered(Box::new(Scan::new(table)), filter)
}

fn filtered<'a>(child: Box<dyn Operator + 'a>, filter: Option<BoundExpr>) -> Box<dyn Operator + 'a> {
    match filter {
        Some(predicate) => Box::new(Filter::new(child, predicate)),
        None => child,
    }
}
