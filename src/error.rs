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

    NoSuchColumn {
        name: String,
    },

    NoSuchTable {
        name: String,
    },

    TableAlreadyExists {
        name: String,
    },
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
                let got = match got {
                    Some(t) => t.to_string(),
                    None => "NULL".to_string(),
                };
                write!(f, "column `{column}` expects {expected}, got {got}")
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
        }
    }
}

impl std::error::Error for DbError {}
pub type Result<T> = std::result::Result<T, DbError>;
