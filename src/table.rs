use crate::error::Result;
use crate::schema::Schema;
use crate::value::Value;
use std::ops::Index;

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
    rows: Vec<Option<Row>>,
    live_count: usize,
}

impl Table {
    pub fn new(name: impl Into<String>, schema: Schema) -> Self {
        Table {
            name: name.into(),
            schema,
            rows: Vec::new(),
            live_count: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn len(&self) -> usize {
        self.live_count
    }

    pub fn is_empty(&self) -> bool {
        self.live_count == 0
    }

    pub fn slot_count(&self) -> usize {
        self.rows.len()
    }

    pub fn insert(&mut self, values: Vec<Value>) -> Result<usize> {
        self.schema.validate_row(&values)?;

        let id = self.rows.len();
        self.rows.push(Some(Row(values)));
        self.live_count += 1;
        Ok(id)
    }

    pub fn update(&mut self, id: usize, values: Vec<Value>) -> Result<bool> {
        self.schema.validate_row(&values)?;
        match self.rows.get_mut(id) {
            Some(slot @ Some(_)) => {
                *slot = Some(Row(values));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn delete(&mut self, id: usize) -> bool {
        match self.rows.get_mut(id) {
            Some(slot) if slot.is_some() => {
                *slot = None;
                self.live_count -= 1;
                true
            }
            _ => false,
        }
    }

    pub fn get(&self, id: usize) -> Option<&Row> {
        self.rows.get(id)?.as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Row> + '_ {
        self.rows.iter().flatten()
    }

    pub fn iter_with_ids(&self) -> impl Iterator<Item = (usize, &Row)> + '_ {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| slot.as_ref().map(|row| (id, row)))
    }

    pub fn compact(&mut self) {
        self.rows.retain(|slot| slot.is_some());
        self.live_count = self.rows.len();
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
        assert_eq!(u.slot_count(), 0)
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
    fn len_counts_live_rows_not_slots() {
        let mut u = seed();
        u.delete(1);
        assert_eq!(u.len(), 2);
        assert_eq!(u.slot_count(), 3);
    }

    #[test]
    fn iteration_skips_tomb_stones() {
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
