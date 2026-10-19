//! Turns a bound SELECT into a Plan: a tree of Operators (ADR-0002).

use crate::binder::BoundSelect;
use crate::operator::{Operator, Project, SingleRow};

pub fn plan_select(select: BoundSelect) -> Box<dyn Operator> {
    // Without FROM, the SELECT list is computed once, over a single empty Row.
    Box::new(Project::new(Box::new(SingleRow::new()), select.exprs))
}
