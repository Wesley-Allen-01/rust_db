use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Int,
    Float,
    Text,
    Bool,
}

impl DataType {
    /// Whether a Value of this Data Type can stand where `target` is needed:
    /// the same Data Type, or INT going to FLOAT (Coercion).
    pub fn coerces_to(self, target: DataType) -> bool {
        self == target || (self, target) == (DataType::Int, DataType::Float)
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            DataType::Int => "INT",
            DataType::Float => "FLOAT",
            DataType::Text => "TEXT",
            DataType::Bool => "BOOL",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    Null,
}

impl Value {
    pub fn type_of(&self) -> Option<DataType> {
        match self {
            Value::Int(_) => Some(DataType::Int),
            Value::Float(_) => Some(DataType::Float),
            Value::Text(_) => Some(DataType::Text),
            Value::Bool(_) => Some(DataType::Bool),
            Value::Null => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn fits(&self, expected: DataType) -> bool {
        match self.type_of() {
            None => true,
            Some(actual) => actual == expected,
        }
    }

    /// This Value as `target`, applying Coercion (INT to FLOAT) if needed.
    /// NULL becomes any Data Type. `None` if the Value can't become `target`.
    pub fn coerce_to(self, target: DataType) -> Option<Value> {
        if !self.type_of().is_none_or(|t| t.coerces_to(target)) {
            return None;
        }
        Some(match (self, target) {
            (Value::Int(i), DataType::Float) => Value::Float(i as f64),
            (value, _) => value,
        })
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Text(s) => write!(f, "{s}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Null => write!(f, "NULL"),
        }
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Text(s.to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_of_reports_variant() {
        assert_eq!(Value::Int(1).type_of(), Some(DataType::Int));
        assert_eq!(Value::Float(2.5).type_of(), Some(DataType::Float));
        assert_eq!(Value::Null.type_of(), None);
    }

    #[test]
    fn null_fits_any_column() {
        assert!(Value::Null.fits(DataType::Int));
        assert!(Value::Null.fits(DataType::Text));
    }

    #[test]
    fn type_mismatch_is_rejected() {
        assert!(Value::Int(1).fits(DataType::Int));
        assert!(!Value::Int(2).fits(DataType::Text));
        assert!(!Value::Float(3.2).fits(DataType::Int));
    }

    // Rust equality is structural, for tests and lookups. SQL comparison
    // (Coercion, NULL is unknown) lives in the evaluator.
    #[test]
    fn rust_equality_is_structural() {
        assert_ne!(Value::Int(1), Value::Float(1.0));
        assert_ne!(Value::Text("1".into()), Value::Int(1));
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(Value::Text("a".into()), Value::Text("a".into()));
    }

    #[test]
    fn coercion_turns_an_int_into_a_float() {
        assert_eq!(
            Value::Int(3).coerce_to(DataType::Float),
            Some(Value::Float(3.0))
        );
    }

    #[test]
    fn coercion_leaves_matching_values_and_null_alone() {
        assert_eq!(Value::Int(3).coerce_to(DataType::Int), Some(Value::Int(3)));
        assert_eq!(Value::Null.coerce_to(DataType::Float), Some(Value::Null));
    }

    #[test]
    fn coercion_is_only_int_to_float() {
        assert_eq!(Value::Float(3.0).coerce_to(DataType::Int), None);
        assert_eq!(Value::Int(1).coerce_to(DataType::Text), None);
        assert_eq!(Value::Text("1".into()).coerce_to(DataType::Int), None);
        assert_eq!(Value::Bool(true).coerce_to(DataType::Int), None);
    }

    #[test]
    fn display_is_bare() {
        assert_eq!(Value::Text("hi".into()).to_string(), "hi");
        assert_eq!(Value::Null.to_string(), "NULL");
        assert_eq!(Value::Bool(true).to_string(), "true");
    }
}
