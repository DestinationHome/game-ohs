use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};

#[derive(Copy, Clone, PartialOrd, PartialEq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Number {
    Positive(u64),
    Negative(i64),
    Float(f64),
}

impl Display for Number {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Positive(x) => write!(f, "{}", x),
            Self::Negative(x) => write!(f, "{}", x),
            Self::Float(x) => write!(f, "{}", x),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Vector(pub Number, pub Number, pub Number, pub Number);

impl Index<usize> for Vector {
    type Output = Number;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.0,
            1 => &self.1,
            2 => &self.2,
            3 => &self.3,
            _ => panic!("unknown vector field: {}", index),
        }
    }
}

impl IndexMut<usize> for Vector {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.0,
            1 => &mut self.1,
            2 => &mut self.2,
            3 => &mut self.3,
            _ => panic!("unknown vector field: {}", index),
        }
    }
}

#[derive(Clone, PartialOrd, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Key {
    Positive(u64),
    Negative(i64),
    String(String),
}

impl TryFrom<Number> for Key {
    type Error = std::io::Error;

    fn try_from(value: Number) -> Result<Self, Self::Error> {
        match value {
            Number::Positive(i) => Ok(Self::Positive(i)),
            Number::Negative(i) => Ok(Self::Negative(i)),
            Number::Float(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "float keys not supported and will never be",
            )),
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TableType {
    Array(Vec<Value>),
    Map(HashMap<Key, Value>),
}

#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(Number),
    String(String),
    Vector(Vector),
    Table(TableType),
    IntArray(Vec<i64>),
}

impl serde::Serialize for Value {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Nil => s.serialize_none(),
            Self::Bool(x) => x.serialize(s),
            Self::Number(x) => x.serialize(s),
            Self::String(str) => str.serialize(s),
            Self::Vector(v) => s.collect_seq([v.0, v.1, v.2, v.3].iter()),
            Self::Table(t) => match t {
                TableType::Array(a) => s.collect_seq(a.iter()),
                TableType::Map(m) => s.collect_map(m.iter()),
            },
            Self::IntArray(a) => s.collect_seq(a.iter()),
        }
    }
}
