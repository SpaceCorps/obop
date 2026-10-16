//! JSON conversion for the network surfaces.

use crate::Value;
use serde_json::{Map, Number, Value as J};

impl Value {
    pub fn from_json(j: &J) -> Value {
        match j {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => n
                .as_i64()
                .map_or_else(|| Value::Text(n.to_string()), Value::Int),
            J::String(s) => Value::Text(s.clone()),
            J::Array(a) => Value::List(a.iter().map(Value::from_json).collect()),
            J::Object(o) => Value::Map(
                o.iter()
                    .map(|(k, v)| (k.clone(), Value::from_json(v)))
                    .collect(),
            ),
        }
    }

    pub fn to_json(&self) -> J {
        match self {
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Int(i) => J::Number(Number::from(*i)),
            Value::Text(t) => J::String(t.clone()),
            Value::List(l) => J::Array(l.iter().map(Value::to_json).collect()),
            Value::Map(m) => J::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect::<Map<_, _>>(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let j: J = serde_json::json!({"a": [1, "x", null, true], "b": {"c": 2}});
        assert_eq!(Value::from_json(&j).to_json(), j);
    }
}
