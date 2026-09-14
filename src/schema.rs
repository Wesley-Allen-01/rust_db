use crate::error::{DbError, Result};
use crate::value::{DataType, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    name: String,
    data_type: DataType,
}

impl Column {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Column {
            name: name.into(),
            data_type: data_type,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn data_type(&self) -> DataType {
        self.data_type
    }

    pub fn accepts(&self, value: &Value) -> bool {
        value.fits(self.data_type)
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

    pub fn validate_row(&self, row: &[Value]) -> Result<()> {
        if row.len() != self.len() {
            return Err(DbError::ArityMismatch {
                expected: self.len(),
                got: row.len(),
            });
        }

        for (col, val) in self.columns.iter().zip(row) {
            if !col.accepts(val) {
                return Err(DbError::TypeMismatch {
                    column: col.name.clone(),
                    expected: col.data_type,
                    got: val.type_of(),
                });
            }
        }

        Ok(())
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
        assert!(u.validate_row(&row).is_ok());
    }

    #[test]
    fn null_is_allowed() {
        let u = users();
        let row = vec![Value::Null, Value::Null, Value::Null, Value::Null];
        assert!(u.validate_row(&row).is_ok());
    }

    #[test]
    fn rejects_wrong_num_vals() {
        let u = users();
        let row = vec![Value::Int(1)];
        assert_eq!(
            u.validate_row(&row).unwrap_err(),
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
            Value::Int(100),
            Value::Bool(false),
        ];

        assert_eq!(
            u.validate_row(&row).unwrap_err(),
            DbError::TypeMismatch {
                column: "score".to_string(),
                expected: DataType::Float,
                got: Some(DataType::Int)
            }
        )
    }
}
