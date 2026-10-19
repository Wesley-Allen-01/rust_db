use crate::error::Result;
use crate::schema::Schema;
use crate::value::Value;
use std::collections::BTreeMap;
use std::ops::Index;

/// A Table's permanent handle for one of its Rows (see GLOSSARY.md).
pub type RowId = usize;

#[derive(Debug, Clone, PartialEq)]
pub struct Row(Vec<Value>);

impl Row {
    pub fn values(&self) -> &[Value] {
        &self.0
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        self.0.get(index)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Index<usize> for Row {
    type Output = Value;

    fn index(&self, index: usize) -> &Value {
        &self.0[index]
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    name: String,
    schema: Schema,
    rows: BTreeMap<usize, Row>,
    next_id: usize,
}

impl Table {
    pub fn new(name: impl Into<String>, schema: Schema) -> Self {
        Table {
            name: name.into(),
            schema,
            rows: BTreeMap::new(),
            next_id: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Applies Coercion, validates the Row, and returns its new Row ID.
    pub fn insert(&mut self, values: Vec<Value>) -> Result<usize> {
        let values = self.schema.coerce_row(values)?;
        Ok(self.push(values))
    }

    /// Inserts every Row or none: all are coerced and validated before any
    /// is added. Returns their Row IDs in order.
    pub fn insert_all(&mut self, rows: Vec<Vec<Value>>) -> Result<Vec<usize>> {
        let rows = rows
            .into_iter()
            .map(|values| self.schema.coerce_row(values))
            .collect::<Result<Vec<_>>>()?;
        Ok(rows.into_iter().map(|values| self.push(values)).collect())
    }

    /// Adds a Row that has already been coerced and validated.
    fn push(&mut self, values: Vec<Value>) -> usize {
        // IDs only ever increase, so a deleted row's ID is never handed out again.
        let id = self.next_id;
        self.next_id += 1;
        self.rows.insert(id, Row(values));
        id
    }

    pub fn update(&mut self, id: usize, values: Vec<Value>) -> Result<bool> {
        let values = self.schema.coerce_row(values)?;
        match self.rows.get_mut(&id) {
            Some(row) => {
                *row = Row(values);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn delete(&mut self, id: usize) -> bool {
        self.rows.remove(&id).is_some()
    }

    pub fn get(&self, id: usize) -> Option<&Row> {
        self.rows.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Row> + '_ {
        self.rows.values()
    }

    pub fn iter_with_ids(&self) -> impl Iterator<Item = (RowId, &Row)> + '_ {
        self.rows.iter().map(|(&id, row)| (id, row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DbError;
    use crate::schema::Column;
    use crate::value::DataType;

    fn users() -> Table {
        let schema = Schema::new(vec![
            Column::new("id", DataType::Int),
            Column::new("name", DataType::Text),
            Column::new("is_active", DataType::Bool),
        ])
        .unwrap();
        Table::new("users", schema)
    }

    fn seed() -> Table {
        let mut u = users();
        u.insert(vec![1.into(), "ada".into(), true.into()]).unwrap();
        u.insert(vec![2.into(), "caleb".into(), false.into()])
            .unwrap();
        u.insert(vec![3.into(), "hannah".into(), true.into()])
            .unwrap();
        u
    }

    #[test]
    fn insert_returns_sequential_ids() {
        let mut u = users();
        assert_eq!(u.insert(vec![1.into(), "caleb".into(), true.into()]), Ok(0));
        assert_eq!(u.insert(vec![2.into(), "zoe".into(), false.into()]), Ok(1));
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn insert_validates_against_schema() {
        let mut u = users();
        let err = u.insert(vec![1.into(), 2.into(), true.into()]).unwrap_err();
        assert_eq!(
            err,
            DbError::TypeMismatch {
                column: "name".to_string(),
                expected: DataType::Text,
                got: Some(DataType::Int)
            }
        );
        assert_eq!(u.len(), 0);
        // A rejected insert must not use up an ID.
        assert_eq!(u.insert(vec![1.into(), "ada".into(), true.into()]), Ok(0));
    }

    fn scores() -> Table {
        let schema = Schema::new(vec![
            Column::new("name", DataType::Text).not_null(),
            Column::new("score", DataType::Float),
        ])
        .unwrap();
        Table::new("scores", schema)
    }

    #[test]
    fn insert_coerces_an_int_into_a_float_column() {
        let mut t = scores();
        let id = t.insert(vec!["ada".into(), 7.into()]).unwrap();
        assert_eq!(t.get(id).unwrap()[1], Value::Float(7.0));
    }

    #[test]
    fn insert_all_inserts_every_row_in_order() {
        let mut t = scores();
        let ids = t
            .insert_all(vec![
                vec!["ada".into(), 1.5.into()],
                vec!["bob".into(), 2.into()],
            ])
            .unwrap();
        assert_eq!(ids, vec![0, 1]);
        assert_eq!(t.get(1).unwrap()[1], Value::Float(2.0));
    }

    #[test]
    fn insert_all_leaves_the_table_unchanged_if_any_row_fails() {
        let mut t = scores();
        let err = t
            .insert_all(vec![
                vec!["ada".into(), 1.5.into()],
                vec!["bob".into(), 2.5.into()],
                vec![Value::Null, 3.5.into()],
            ])
            .unwrap_err();
        assert_eq!(
            err,
            DbError::NotNullViolation {
                column: "name".to_string()
            }
        );
        assert_eq!(t.len(), 0);
        // No IDs were used up either.
        assert_eq!(t.insert(vec!["cy".into(), Value::Null]), Ok(0));
    }

    #[test]
    fn delete_keeps_later_ids_stable() {
        let mut u = seed();
        assert!(u.delete(1));

        assert_eq!(u.get(2).unwrap()[1], Value::Text("hannah".into()));
        assert!(u.get(1).is_none());
    }

    #[test]
    fn delete_is_idempotent() {
        let mut u = seed();
        assert!(u.delete(1));
        assert!(!u.delete(1));
        assert!(!u.delete(99));
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn len_counts_live_rows() {
        let mut u = seed();
        u.delete(1);
        assert_eq!(u.len(), 2);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut u = seed();
        assert!(u.delete(2));

        let id = u.insert(vec![4.into(), "zoe".into(), true.into()]).unwrap();
        assert_eq!(id, 3);
        assert!(u.get(2).is_none());
        assert_eq!(u.get(3).unwrap()[1], Value::Text("zoe".into()));
    }

    #[test]
    fn iteration_skips_deleted_rows() {
        let mut u = seed();
        u.delete(1);

        let names: Vec<&str> = u.iter().map(|r| r[1].as_text().unwrap()).collect();
        assert_eq!(names, vec!["ada", "hannah"]);

        let ids: Vec<usize> = u.iter_with_ids().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![0, 2]);
    }

    #[test]
    fn update_performs_modification_in_place() {
        let mut u = seed();
        assert_eq!(
            u.update(1, vec![1.into(), "jonny".into(), true.into()]),
            Ok(true)
        );
    }
}
