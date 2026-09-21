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
            .get(&Self::normalize(name))
            .ok_or_else(|| DbError::NoSuchTable {
                name: name.to_string(),
            })
    }

    pub fn get_mut(&mut self, name: &str) -> Result<&mut Table> {
        self.tables
            .get_mut(&Self::normalize(name))
            .ok_or_else(|| DbError::NoSuchTable {
                name: name.to_string(),
            })
    }

    pub fn try_get(&self, name: &str) -> Option<&Table> {
        self.tables.get(&Self::normalize(name))
    }

    pub fn try_get_mut(&mut self, name: &str) -> Option<&mut Table> {
        self.tables.get_mut(&Self::normalize(name))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Table> + '_ {
        self.tables.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Column;
    use crate::value::{DataType, Value};

    pub fn users_schema() -> Schema {
        Schema::new(vec![
            Column::new("id", DataType::Int),
            Column::new("name", DataType::Text),
            Column::new("email", DataType::Text),
        ])
        .unwrap()
    }

    fn db() -> Database {
        let mut db = Database::new();
        db.create_table("users", users_schema()).unwrap();
        db
    }

    #[test]
    fn creates_and_finds_table() {
        let db = db();
        assert_eq!(db.len(), 1);
        assert!(db.contains_table("users"));
        assert_eq!(db.get("users").unwrap().name(), "users");
    }

    #[test]
    fn table_names_are_case_insensitive() {
        let db = db();
        assert!(db.contains_table("users"));
        assert!(db.contains_table("USERS"));
        assert!(db.get("Users").is_ok());
    }

    #[test]
    fn original_spelling_is_preserved() {
        let mut db = Database::new();
        db.create_table("UserAccounts", users_schema()).unwrap();
        assert!(db.contains_table("useraccounts"));
        assert_eq!(db.get("useraccounts").unwrap().name(), "UserAccounts");
        assert_eq!(db.table_names(), vec!["UserAccounts"]);
    }
}
