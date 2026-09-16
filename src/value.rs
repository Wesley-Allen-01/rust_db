use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Int,
    Float,
    Text,
    Bool,
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

#[derive(Debug, Clone)]
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

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Value::Null, _) | (_, Value::Null) => None,

            (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),

            (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
            (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),

            (Value::Text(a), Value::Text(b)) => a.partial_cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),

            _ => None,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Null, _) | (_, Value::Null) => false,
            _ => self.partial_cmp(other) == Some(Ordering::Equal),
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
        assert_eq!(Value::Float(3.14).type_of(), Some(DataType::Float));
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

    #[test]
    fn numerics_cmp_across_int_and_float() {
        assert_eq!(Value::Int(1), Value::Float(1.0));
        assert!(Value::Float(3.4) > Value::Int(3));
    }

    #[test]
    fn different_types_are_incompatible() {
        let s = Value::Text("hello".into());
        let i = Value::Int(3);

        assert_eq!(s.partial_cmp(&i), None);
        assert!(s != i);
        assert!(!((s <= i) && (i <= s)));
    }

    #[test]
    fn null_is_never_ordered() {
        assert_eq!(Value::Null.partial_cmp(&Value::Null), None);
        assert_eq!(Value::Null.partial_cmp(&Value::Int(1)), None);
    }

    #[test]
    fn display_is_bare() {
        assert_eq!(Value::Text("hi".into()).to_string(), "hi");
        assert_eq!(Value::Null.to_string(), "NULL");
        assert_eq!(Value::Bool(true).to_string(), "true");
    }
}
