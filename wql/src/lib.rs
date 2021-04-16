use chrono::{DateTime, Utc};
use language_parser::read_symbol;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{cmp::Ordering, hash::Hash};
use std::{collections::HashMap, str::FromStr};
use uuid::Uuid;
mod join;
mod language_parser;
mod logic;
mod relation;
mod select;
#[cfg(test)]
mod test;
mod where_clause;

pub use logic::parse_value as parse_types;
use logic::{integer_decode, read_map, read_match_args};
pub use relation::{Relation, RelationType};
pub use where_clause::{Clause, Function, Value};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Wql {
    CreateEntity(String, Vec<String>, Vec<String>),
    Insert(String, Entity, Option<ID>),
    UpdateContent(String, Entity, ID),
    UpdateSet(String, Entity, ID),
    Delete(String, String),
    MatchUpdate(String, Entity, ID, MatchCondition),
    Evict(String, Option<ID>),
    Select(String, ToSelect, Option<ID>, HashMap<String, Algebra>),
    SelectWhen(String, ToSelect, Option<ID>, String),
    SelectWhenRange(String, ID, String, String),
    SelectIds(String, ToSelect, Vec<ID>, HashMap<String, Algebra>),
    SelectWhere(String, ToSelect, Vec<Clause>, HashMap<String, Algebra>),
    CheckValue(String, ID, HashMap<String, String>),
    RelationQuery(Vec<Wql>, Relation, RelationType),
    Join((String, String), (String, String), Vec<Wql>),
}

pub use select::{Algebra, Order};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ToSelect {
    All,
    Keys(Vec<String>),
}

pub type Entity = HashMap<String, Types>;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum MatchCondition {
    All(Vec<MatchCondition>),
    Any(Vec<MatchCondition>),
    Eq(String, Types),
    NotEq(String, Types),
    GEq(String, Types),
    G(String, Types),
    LEq(String, Types),
    L(String, Types),
}

pub(crate) fn tokenize(wql: &str) -> std::str::Chars {
    wql.chars()
}

impl std::str::FromStr for Wql {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tokens = tokenize(s.trim_start());
        let wql = parse(tokens.next(), &mut tokens)?;
        Ok(wql)
    }
}

pub(crate) fn parse(c: Option<char>, chars: &mut std::str::Chars) -> Result<Wql, String> {
    c.map_or_else(
        || Err(String::from("Empty WQL")),
        |ch| read_symbol(ch, chars),
    )
}

#[allow(clippy::derive_hash_xor_eq)] // for now
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Types {
    Char(char),
    Integer(isize),
    String(String),
    Uuid(Uuid),
    Float(f64),
    Boolean(bool),
    Vector(Vec<Types>),
    Map(HashMap<String, Types>),
    Hash(String),
    Precise(String),
    DateTime(DateTime<Utc>),
    Nil,
}

impl Types {
    pub fn default_values(&self) -> Types {
        match self {
            Types::Char(_) => Types::Char(' '),
            Types::Integer(_) => Types::Integer(0),
            Types::String(_) => Types::String(String::new()),
            Types::Uuid(_) => Types::Uuid(Uuid::new_v4()),
            Types::Float(_) => Types::Float(0_f64),
            Types::Boolean(_) => Types::Boolean(false),
            Types::Vector(_) => Types::Vector(Vec::new()),
            Types::Map(_) => Types::Map(HashMap::new()),
            Types::Hash(_) => Types::Hash(String::new()),
            Types::Precise(_) => Types::Precise(String::from("0")),
            Types::DateTime(_) => Types::DateTime(Utc::now()),
            Types::Nil => Types::Nil,
        }
    }

    pub fn to_hash(&self, cost: Option<u32>) -> Result<Types, String> {
        use bcrypt::{hash, DEFAULT_COST};
        let value = match self {
            Types::Char(c) => format!("{}", c),
            Types::Integer(i) => format!("{}", i),
            Types::String(s) => s.to_string(),
            Types::DateTime(date) => date.to_string(),
            Types::Uuid(id) => format!("{}", id),
            Types::Float(f) => format!("{:?}", integer_decode(f.to_owned())),
            Types::Boolean(b) => format!("{}", b),
            Types::Vector(vec) => format!("{:?}", vec),
            Types::Map(map) => format!("{:?}", map),
            Types::Precise(p) => p.to_string(),
            Types::Hash(_) => return Err(String::from("Hash cannot be hashed")),
            Types::Nil => return Err(String::from("Nil cannot be hashed")),
        };
        match hash(&value, cost.map_or(DEFAULT_COST, |c| c)) {
            Ok(s) => Ok(Types::Hash(s)),
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    pub fn is_hash(&self) -> bool {
        matches!(self, Types::Hash(_))
    }
}

impl Eq for Types {}
impl PartialOrd for Types {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Types::Integer(a), Types::Integer(b)) => Some(a.cmp(b)),

            (Types::Float(a), Types::Float(b)) => Some(if a > b {
                Ordering::Greater
            } else {
                Ordering::Less
            }),
            (Types::Integer(a), Types::Float(b)) => Some(if &(*a as f64) > b {
                Ordering::Greater
            } else {
                Ordering::Less
            }),
            (Types::Float(a), Types::Integer(b)) => Some(if a > &(*b as f64) {
                Ordering::Greater
            } else {
                Ordering::Less
            }),
            (Types::Char(a), Types::Char(b)) => Some(a.cmp(b)),
            (Types::String(a), Types::String(b)) | (Types::Precise(a), Types::Precise(b)) => {
                Some(a.cmp(b))
            }
            (Types::Uuid(a), Types::Uuid(b)) => Some(a.cmp(b)),
            (Types::Boolean(a), Types::Boolean(b)) => Some(a.cmp(b)),
            (Types::Vector(a), Types::Vector(b)) => Some(a.len().cmp(&b.len())),
            _ => None,
        }
    }
}

// UNSAFE
#[allow(clippy::derive_hash_xor_eq)] // for now
impl Hash for Types {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Types::Char(t) => t.hash(state),
            Types::Integer(t) => t.hash(state),
            Types::String(t) => t.hash(state),
            Types::Uuid(t) => t.hash(state),
            Types::Float(t) => {
                let int_t = integer_decode(t.to_owned());
                int_t.hash(state)
            }
            Types::Boolean(t) => t.hash(state),
            Types::Vector(t) => t.hash(state),
            Types::Map(t) => t.into_iter().fold((), |acc, (k, v)| {
                k.hash(state);
                v.hash(state);
                acc
            }),
            Types::Hash(t) => t.hash(state),
            Types::Precise(t) => t.hash(state),
            Types::DateTime(t) => t.hash(state),
            Types::Nil => "".hash(state),
        }
    }
}

#[derive(PartialEq, PartialOrd, Eq, Ord, Clone, Debug)]
pub enum ID {
    Uuid(Uuid),
    Number(usize),
    String(String),
}

impl Serialize for ID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ID::Uuid(id) => {
                if serializer.is_human_readable() {
                    serializer.serialize_str(&id.to_hyphenated().encode_lower(&mut [0; 36]))
                } else {
                    serializer.serialize_bytes(id.as_bytes())
                }
            }
            ID::Number(num) => serializer.serialize_u64(*num as u64),
            ID::String(s) => serializer.serialize_str(s),
        }
    }
}

use std::fmt;

use serde::de::{self, Visitor};

struct IDVisitor;

impl<'de> Visitor<'de> for IDVisitor {
    type Value = ID;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an id must be usize, uuid or String")
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if Uuid::parse_str(&v).is_ok() {
            Ok(ID::Uuid(Uuid::parse_str(&v).unwrap()))
        } else {
            Ok(ID::String(v))
        }
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        use std::u64;
        if value >= u64::from(u64::MIN) && value <= u64::from(u64::MAX) {
            Ok(ID::Number(value as usize))
        } else {
            Err(E::custom(format!("u64 out of range: {}", value)))
        }
    }
}

impl<'de> Deserialize<'de> for ID {
    fn deserialize<D>(deserializer: D) -> Result<ID, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(IDVisitor)
    }
}

impl ID {
    pub fn new() -> ID {
        let uuid = Uuid::new_v4();
        ID::Uuid(uuid)
    }

    pub fn new_with_usize(number: usize) -> ID {
        ID::Number(number)
    }

    pub fn new_with_str(s: &str) -> ID {
        ID::String(s.to_owned())
    }

    pub fn to_string(&self) -> String {
        match self {
            ID::Number(num) => num.to_string(),
            ID::String(s) => s.to_owned(),
            ID::Uuid(id) => id.to_string(),
        }
    }
}

impl FromStr for ID {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if Uuid::parse_str(s).is_ok() {
            Ok(ID::Uuid(Uuid::parse_str(s).unwrap()))
        } else if s.parse::<usize>().is_ok() {
            Ok(ID::Number(s.parse::<usize>().unwrap()))
        } else if s.is_empty() {
            Err(String::from("Entity cannot be empty in EVICT"))
        } else {
            Ok(ID::String(s.to_owned()))
        }
    }
}
