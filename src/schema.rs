use crate::error::{DbError, Result};
use crate::value::{DataType, Value};

/// A Result Set Column: its name and Data Type, known before any Row runs.
/// Unlike a Table's Column, it carries no Constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultColumn {
    pub name: String,
    pub data_type: DataType,
}

impl ResultColumn {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        ResultColumn {
            name: name.into(),
            data_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    name: String,
    data_type: DataType,
    not_null: bool,
}

impl Column {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Column {
            name: name.into(),
            data_type,
            not_null: false,
        }
    }

    /// Adds the NOT NULL Constraint. Columns are nullable by default.
    pub fn not_null(mut self) -> Self {
        self.not_null = true;
        self
    }

    pub fn is_not_null(&self) -> bool {
        self.not_null
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn data_type(&self) -> DataType {
        self.data_type
    }

    /// `value` as this Column's Data Type, applying Coercion, or the
    /// Constraint or Data Type it breaks.
    pub fn coerce(&self, value: Value) -> Result<Value> {
        if self.not_null && value.is_null() {
            return Err(DbError::NotNullViolation {
                column: self.name.clone(),
            });
        }
        let got = value.type_of();
        value
            .coerce_to(self.data_type)
            .ok_or_else(|| DbError::TypeMismatch {
                column: self.name.clone(),
                expected: self.data_type,
                got,
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    columns: Vec<Column>,
}

impl Schema {
    pub fn new(columns: Vec<Column>) -> Result<Self> {
        if columns.is_empty() {
            return Err(DbError::EmptySchema);
        }

        for (i, column) in columns.iter().enumerate() {
            for earlier in &columns[..i] {
                if earlier.name.eq_ignore_ascii_case(&column.name) {
                    return Err(DbError::DuplicateColumn {
                        name: column.name.clone(),
                    });
                }
            }
        }

        Ok(Schema { columns })
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn column(&self, index: usize) -> Option<&Column> {
        self.columns.get(index)
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|x| x.name.eq_ignore_ascii_case(name))
    }

    pub fn column_by_name(&self, name: &str) -> Option<&Column> {
        self.column_index(name).and_then(|i| self.column(i))
    }

    pub fn resolve(&self, name: &str) -> Result<usize> {
        self.column_index(name)
            .ok_or_else(|| DbError::NoSuchColumn {
                name: name.to_string(),
            })
    }

    /// Checks a Row against the Schema and returns it with Coercion applied:
    /// arity first, then NOT NULL and Data Type for each Column in turn.
    pub fn coerce_row(&self, row: Vec<Value>) -> Result<Vec<Value>> {
        if row.len() != self.len() {
            return Err(DbError::ArityMismatch {
                expected: self.len(),
                got: row.len(),
            });
        }
        row.into_iter()
            .zip(&self.columns)
            .map(|(value, col)| col.coerce(value))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users() -> Schema {
        Schema::new(vec![
            Column::new("id", DataType::Int),
            Column::new("name", DataType::Text),
            Column::new("score", DataType::Float),
            Column::new("active", DataType::Bool),
        ])
        .unwrap()
    }

    #[test]
    fn rejects_empty_schema() {
        let result = Schema::new(vec![]);
        assert!(matches!(result, Err(DbError::EmptySchema)));
    }

    #[test]
    fn rejects_duplicate_names_case_insensitive() {
        let result = Schema::new(vec![
            Column::new("id", DataType::Int),
            Column::new("ID", DataType::Text),
        ]);
        assert!(matches!(result, Err(DbError::DuplicateColumn { name }) if name == "ID"));
    }

    #[test]
    fn looks_up_col_by_name() {
        let u = users();

        assert_eq!(u.column_index("name"), Some(1));
        assert_eq!(u.column_index("NAME"), Some(1));
        assert_eq!(u.column_index("test"), None);
    }

    #[test]
    fn resolve_reports_missing_cols() {
        let us = users();
        assert_eq!(
            us.resolve("test").unwrap_err(),
            DbError::NoSuchColumn {
                name: "test".to_string()
            }
        );
    }

    #[test]
    fn accepts_well_formed_row() {
        let u = users();
        let row = vec![
            Value::Int(1),
            Value::Text("Wesley".into()),
            Value::Float(99.9),
            Value::Bool(true),
        ];
        assert!(u.coerce_row(row).is_ok());
    }

    #[test]
    fn null_is_allowed() {
        let u = users();
        let row = vec![Value::Null, Value::Null, Value::Null, Value::Null];
        assert!(u.coerce_row(row).is_ok());
    }

    #[test]
    fn rejects_wrong_num_vals() {
        let u = users();
        let row = vec![Value::Int(1)];
        assert_eq!(
            u.coerce_row(row).unwrap_err(),
            DbError::ArityMismatch {
                expected: 4,
                got: 1
            }
        );
    }

    #[test]
    fn rejects_wrong_type() {
        let u = users();
        let row = vec![
            Value::Int(1),
            Value::Text("Wesley".into()),
            Value::Text("high".into()),
            Value::Bool(false),
        ];

        assert_eq!(
            u.coerce_row(row).unwrap_err(),
            DbError::TypeMismatch {
                column: "score".to_string(),
                expected: DataType::Float,
                got: Some(DataType::Text)
            }
        )
    }

    #[test]
    fn coerce_row_turns_an_int_into_a_float_column_into_a_float() {
        let row = vec![
            Value::Int(1),
            Value::Text("Wesley".into()),
            Value::Int(100),
            Value::Bool(false),
        ];
        assert_eq!(
            users().coerce_row(row),
            Ok(vec![
                Value::Int(1),
                Value::Text("Wesley".into()),
                Value::Float(100.0),
                Value::Bool(false),
            ])
        );
    }

    #[test]
    fn coerce_row_rejects_extra_values() {
        let row = vec![Value::Null; 5];
        assert_eq!(
            users().coerce_row(row).unwrap_err(),
            DbError::ArityMismatch {
                expected: 4,
                got: 5
            }
        );
    }

    #[test]
    fn coerce_row_still_rejects_other_mismatches() {
        let row = vec![Value::Float(1.0), Value::Null, Value::Null, Value::Null];
        assert_eq!(
            users().coerce_row(row).unwrap_err(),
            DbError::TypeMismatch {
                column: "id".to_string(),
                expected: DataType::Int,
                got: Some(DataType::Float)
            }
        );
    }

    fn accounts() -> Schema {
        Schema::new(vec![
            Column::new("id", DataType::Int).not_null(),
            Column::new("nickname", DataType::Text),
        ])
        .unwrap()
    }

    #[test]
    fn rejects_null_in_not_null_column() {
        let row = vec![Value::Null, Value::Text("ada".into())];
        assert_eq!(
            accounts().coerce_row(row).unwrap_err(),
            DbError::NotNullViolation {
                column: "id".to_string()
            }
        );
    }

    #[test]
    fn not_null_column_accepts_a_value_and_others_still_take_null() {
        let row = vec![Value::Int(1), Value::Null];
        assert!(accounts().coerce_row(row).is_ok());
    }

    #[test]
    fn columns_are_nullable_unless_marked() {
        let schema = accounts();
        assert!(schema.columns()[0].is_not_null());
        assert!(!schema.columns()[1].is_not_null());
    }
}
