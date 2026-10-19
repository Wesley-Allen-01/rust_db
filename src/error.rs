use crate::ast::{BinaryOp, UnaryOp};
use crate::lexer::Position;
use crate::value::DataType;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum DbError {
    DuplicateColumn {
        name: String,
    },

    EmptySchema,

    ArityMismatch {
        expected: usize,
        got: usize,
    },

    TypeMismatch {
        column: String,
        expected: DataType,
        got: Option<DataType>,
    },

    /// A NULL was given for a Column with the NOT NULL Constraint.
    NotNullViolation {
        column: String,
    },

    NoSuchColumn {
        name: String,
    },

    NoSuchTable {
        name: String,
    },

    TableAlreadyExists {
        name: String,
    },

    /// The SQL text is malformed: a lexing or parsing error at `pos`.
    Syntax {
        pos: Position,
        message: String,
    },

    /// A binary operator was given operands of Data Types it doesn't accept.
    /// `None` is an operand that is only known to be NULL.
    BinaryTypeMismatch {
        op: BinaryOp,
        left: Option<DataType>,
        right: Option<DataType>,
    },

    UnaryTypeMismatch {
        op: UnaryOp,
        operand: Option<DataType>,
    },

    /// `SELECT *` without a FROM Table to expand it.
    WildcardWithoutTable,

    /// A WHERE whose Data Type isn't BOOL.
    WhereNotBool {
        got: DataType,
    },

    IntegerOverflow,

    FloatOverflow,

    DivisionByZero,

    NotYetSupported,
}

/// A Data Type for an error message, where `None` means NULL.
fn type_name(data_type: &Option<DataType>) -> String {
    match data_type {
        Some(t) => t.to_string(),
        None => "NULL".to_string(),
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::DuplicateColumn { name } => {
                write!(f, "Duplicate column name: `{name}`")
            }

            DbError::EmptySchema => {
                write!(f, "A table must have at least one column")
            }

            DbError::ArityMismatch { expected, got } => {
                write!(f, "Expected {expected} value(s), got {got}")
            }

            DbError::TypeMismatch {
                column,
                expected,
                got,
            } => {
                let got = type_name(got);
                write!(f, "column `{column}` expects {expected}, got {got}")
            }

            DbError::NotNullViolation { column } => {
                write!(f, "column `{column}` is NOT NULL, got NULL")
            }

            DbError::NoSuchColumn { name } => {
                write!(f, "no such column `{name}`")
            }

            DbError::NoSuchTable { name } => {
                write!(f, "no such table `{name}`")
            }

            DbError::TableAlreadyExists { name } => {
                write!(f, "table `{name}` already exists")
            }

            DbError::Syntax { pos, message } => {
                write!(f, "line {}, col {}: {message}", pos.line, pos.col)
            }

            DbError::BinaryTypeMismatch { op, left, right } => {
                let (left, right) = (type_name(left), type_name(right));
                write!(f, "operator {op} cannot be applied to {left} and {right}")
            }

            DbError::UnaryTypeMismatch { op, operand } => {
                let operand = type_name(operand);
                write!(f, "operator {op} cannot be applied to {operand}")
            }

            DbError::WildcardWithoutTable => {
                write!(f, "SELECT * needs a table to read from")
            }

            DbError::WhereNotBool { got } => {
                write!(f, "WHERE must be BOOL, got {got}")
            }

            DbError::IntegerOverflow => write!(f, "integer out of range"),

            DbError::FloatOverflow => write!(f, "float out of range"),

            DbError::DivisionByZero => write!(f, "division by zero"),

            DbError::NotYetSupported => {
                write!(f, "not yet supported")
            }
        }
    }
}

impl std::error::Error for DbError {}
pub type Result<T> = std::result::Result<T, DbError>;
