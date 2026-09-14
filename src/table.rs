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
            _ => Ok(false)  
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


