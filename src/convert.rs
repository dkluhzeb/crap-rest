use prost_types::value::Kind;
use prost_types::{ListValue, Struct, Value};
use serde_json::{Map, Number};

/// Convert a prost_types::Struct to a serde_json::Value (Object).
pub fn struct_to_json(s: &Struct) -> serde_json::Value {
    let mut map = Map::new();
    for (k, v) in &s.fields {
        map.insert(k.clone(), value_to_json(v));
    }
    serde_json::Value::Object(map)
}

/// Convert a prost_types::Value to a serde_json::Value.
fn value_to_json(v: &Value) -> serde_json::Value {
    match &v.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => {
            // Try integer first for cleaner output
            if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                serde_json::Value::Number(Number::from(*n as i64))
            } else {
                Number::from_f64(*n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
        }
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::StructValue(s)) => struct_to_json(s),
        Some(Kind::ListValue(list)) => list_to_json(list),
        None => serde_json::Value::Null,
    }
}

/// Convert a prost_types::ListValue to a serde_json::Value (Array).
fn list_to_json(list: &ListValue) -> serde_json::Value {
    serde_json::Value::Array(list.values.iter().map(value_to_json).collect())
}

/// Convert a serde_json::Value to a prost_types::Struct.
/// Returns None if the value is not an object.
pub fn json_to_struct(val: &serde_json::Value) -> Option<Struct> {
    match val {
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Some(Struct { fields })
        }
        _ => None,
    }
}

/// Convert a serde_json::Value to a prost_types::Value.
fn json_to_value(val: &serde_json::Value) -> Value {
    let kind = match val {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => Kind::ListValue(ListValue {
            values: arr.iter().map(json_to_value).collect(),
        }),
        serde_json::Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Kind::StructValue(Struct { fields })
        }
    };
    Value { kind: Some(kind) }
}

/// Convert a proto Document to a flat JSON object.
/// Flattens `fields` into the top level alongside id, collection, timestamps.
pub fn document_to_json(doc: &crate::proto::Document) -> serde_json::Value {
    let mut map = Map::new();
    map.insert("id".to_string(), serde_json::Value::String(doc.id.clone()));
    map.insert(
        "collection".to_string(),
        serde_json::Value::String(doc.collection.clone()),
    );

    // Flatten fields into top level
    if let Some(ref fields) = doc.fields {
        for (k, v) in &fields.fields {
            map.insert(k.clone(), value_to_json(v));
        }
    }

    if let Some(ref ts) = doc.created_at {
        map.insert(
            "created_at".to_string(),
            serde_json::Value::String(ts.clone()),
        );
    }
    if let Some(ref ts) = doc.updated_at {
        map.insert(
            "updated_at".to_string(),
            serde_json::Value::String(ts.clone()),
        );
    }

    serde_json::Value::Object(map)
}
