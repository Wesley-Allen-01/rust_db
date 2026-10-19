//! The parsed shape of a Statement, before any names or Data Types are checked.

use crate::value::{DataType, Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Select(Select),
    CreateTable(CreateTable),
    DropTable(DropTable),
    Insert(Insert),
}

/// `INSERT INTO table [(column, ...)] VALUES (expr, ...), ...`.
#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    pub table: String,
    /// `None` when there is no column list: each row then gives every Column,
    /// in Schema order.
    pub columns: Option<Vec<String>>,
    pub rows: Vec<Vec<Expr>>,
}

/// `CREATE TABLE [IF NOT EXISTS] name (column, ...)`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateTable {
    pub name: String,
    pub columns: Vec<ColumnDef>,
    pub if_not_exists: bool,
}

/// One Column in a CREATE TABLE: `name TYPE [NOT NULL]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
    pub not_null: bool,
}

/// `DROP TABLE [IF EXISTS] name`.
#[derive(Debug, Clone, PartialEq)]
pub struct DropTable {
    pub name: String,
    pub if_exists: bool,
}

/// `SELECT item, ... [FROM table] [WHERE expr]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Select {
    pub items: Vec<SelectItem>,
    pub from: Option<String>,
    pub filter: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    /// `*`: every Column of the FROM Table.
    Wildcard,
    /// One output expression, with its optional `AS` name.
    Expr { expr: Expr, alias: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    /// A bare name. The binder resolves it against the Columns in scope.
    Column(String),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// `expr IS NULL`, or `expr IS NOT NULL` when `negated`.
    IsNull {
        operand: Box<Expr>,
        negated: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Concat,
}

impl BinaryOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Eq
                | BinaryOp::NotEq
                | BinaryOp::Lt
                | BinaryOp::LtEq
                | BinaryOp::Gt
                | BinaryOp::GtEq
        )
    }
}

/// The expression as SQL text, used to name an unaliased Result Set Column.
/// Every compound operand is parenthesised, so the text never depends on
/// operator precedence.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(Value::Text(s)) => write!(f, "'{}'", s.replace('\'', "''")),
            // Debug keeps the `.0` that tells `1.0` from `1`.
            Expr::Literal(Value::Float(x)) => write!(f, "{x:?}"),
            Expr::Literal(value) => write!(f, "{value}"),
            Expr::Column(name) => f.write_str(name),
            Expr::Unary {
                op: UnaryOp::Neg,
                operand,
            } => write!(f, "-{}", Operand(operand)),
            Expr::Unary {
                op: UnaryOp::Not,
                operand,
            } => write!(f, "NOT {}", Operand(operand)),
            Expr::Binary { op, left, right } => {
                write!(f, "{} {op} {}", Operand(left), Operand(right))
            }
            Expr::IsNull { operand, negated } => {
                let not = if *negated { " NOT" } else { "" };
                write!(f, "{} IS{not} NULL", Operand(operand))
            }
        }
    }
}

/// An operand inside a larger expression: parenthesised unless it is a
/// literal or a Column name.
struct Operand<'a>(&'a Expr);

impl fmt::Display for Operand<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Expr::Literal(_) | Expr::Column(_) => write!(f, "{}", self.0),
            compound => write!(f, "({compound})"),
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "NOT",
        })
    }
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Eq => "=",
            BinaryOp::NotEq => "<>",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "AND",
            BinaryOp::Or => "OR",
            BinaryOp::Concat => "||",
        })
    }
}
