use crate::error::{DbError, Result};
use crate::schema::{self, Schema};
use crate::table::Table;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Database {
    tables: HashMap<String, Table>,
}

impl Database {
    pub fn new() -> Self {
        Database::default()
    }

    fn normalize(name: &str) -> String {
        name.to_ascii_lowercase()
    }

    pub fn create_table(&mut self, name: impl Into<String>, schema: Schema) -> Result<()> {
        let name = name.into();
        let key = Self::normalize(&name);

        if self.tables.contains_key(&key) {
            return Err(DbError::TableAlreadyExists { name });
        }

        self.tables.insert(key, Table::new(name, schema));
        Ok(())
    }

    pub fn drop_table(&mut self, name: &str) -> Result<Table> {
        self.tables
            .remove(&Self::normalize(name))
            .ok_or_else(|| DbError::NoSuchTable {
                name: name.to_string(),
            })
    }

    pub fn contains_table(&self, name: &str) -> bool {
        self.tables.contains_key(&Self::normalize(name))
    }

    pub fn len(&self) -> usize {
        self.tables.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    pub fn table_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tables.values().map(|t| t.name()).collect();
        names.sort_unstable();
        names
    }

    pub fn get(&self, name: &str) -> Result<&Table> {
        self.tables
            .get(Self::normalize(&name))
            .ok_or_else(|| DbError::NoSuchTable { name: name.to_string() })
    }
    
}
