use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Decimal(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Binary {
    Complete {
        data: String,
        media_type: Option<String>,
        digest: Option<String>,
    },
    Preview {
        byte_count: u64,
        bytes: String,
        media_type: Option<String>,
        digest: Option<String>,
    },
}

impl Binary {
    pub fn byte_count(&self) -> u64 {
        match self {
            Binary::Complete { data, .. } => data.len() as u64,
            Binary::Preview { byte_count, .. } => *byte_count,
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, Binary::Complete { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectField {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductValue {
    pub type_name: String,
    pub rendered: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum Value {
    Missing,
    Null,
    Boolean(bool),
    SignedInteger(i64),
    UnsignedInteger(u64),
    Decimal(Decimal),
    FloatingPoint(f64),
    String(String),
    Binary(Binary),
    Date(String),
    Time(String),
    Timestamp(String),
    Uuid(Uuid),
    Array(Vec<Value>),
    Object(Vec<ObjectField>),
    ProductSpecific(ProductValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    Missing,
    Null,
    Boolean,
    SignedInteger,
    UnsignedInteger,
    Decimal,
    FloatingPoint,
    String,
    Binary,
    Date,
    Time,
    Timestamp,
    Uuid,
    Array,
    Object,
    ProductSpecific,
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        match self {
            Value::Missing => ValueKind::Missing,
            Value::Null => ValueKind::Null,
            Value::Boolean(_) => ValueKind::Boolean,
            Value::SignedInteger(_) => ValueKind::SignedInteger,
            Value::UnsignedInteger(_) => ValueKind::UnsignedInteger,
            Value::Decimal(_) => ValueKind::Decimal,
            Value::FloatingPoint(_) => ValueKind::FloatingPoint,
            Value::String(_) => ValueKind::String,
            Value::Binary(_) => ValueKind::Binary,
            Value::Date(_) => ValueKind::Date,
            Value::Time(_) => ValueKind::Time,
            Value::Timestamp(_) => ValueKind::Timestamp,
            Value::Uuid(_) => ValueKind::Uuid,
            Value::Array(_) => ValueKind::Array,
            Value::Object(_) => ValueKind::Object,
            Value::ProductSpecific(_) => ValueKind::ProductSpecific,
        }
    }

    pub fn render(&self, limit: usize) -> String {
        let rendered = match self {
            Value::Missing => "-".to_string(),
            Value::Null => "NULL".to_string(),
            Value::Boolean(value) => value.to_string(),
            Value::SignedInteger(value) => value.to_string(),
            Value::UnsignedInteger(value) => value.to_string(),
            Value::Decimal(value) => value.0.clone(),
            Value::FloatingPoint(value) => value.to_string(),
            Value::String(value) => value.clone(),
            Value::Binary(binary) => format!(
                "{} bytes{}",
                binary.byte_count(),
                if binary.is_complete() {
                    ""
                } else {
                    " (preview)"
                }
            ),
            Value::Date(value) | Value::Time(value) | Value::Timestamp(value) => value.clone(),
            Value::Uuid(value) => value.to_string(),
            Value::Array(values) => format!("[{} items]", values.len()),
            Value::Object(fields) => format!("{{{} fields}}", fields.len()),
            Value::ProductSpecific(value) => value.rendered.clone(),
        };
        truncate(&rendered, limit)
    }

    pub fn from_json(json: &serde_json::Value) -> Value {
        match json {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(value) => Value::Boolean(*value),
            serde_json::Value::Number(number) => {
                if let Some(value) = number.as_i64() {
                    Value::SignedInteger(value)
                } else if let Some(value) = number.as_u64() {
                    Value::UnsignedInteger(value)
                } else {
                    Value::FloatingPoint(number.as_f64().unwrap_or_default())
                }
            }
            serde_json::Value::String(value) => Value::String(value.clone()),
            serde_json::Value::Array(values) => {
                Value::Array(values.iter().map(Value::from_json).collect())
            }
            serde_json::Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(name, value)| ObjectField {
                        name: name.clone(),
                        value: Value::from_json(value),
                    })
                    .collect(),
            ),
        }
    }
}

pub fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_null_are_different_facts() {
        assert_ne!(Value::Missing, Value::Null);
        assert_ne!(Value::Missing.kind(), Value::Null.kind());
        assert_eq!(Value::Missing.render(20), "-");
        assert_eq!(Value::Null.render(20), "NULL");
    }

    #[test]
    fn every_case_reports_its_own_kind() {
        let cases = [
            (Value::Missing, ValueKind::Missing),
            (Value::Null, ValueKind::Null),
            (Value::Boolean(true), ValueKind::Boolean),
            (Value::SignedInteger(-1), ValueKind::SignedInteger),
            (Value::UnsignedInteger(1), ValueKind::UnsignedInteger),
            (Value::Decimal(Decimal("1.00".into())), ValueKind::Decimal),
            (Value::FloatingPoint(1.5), ValueKind::FloatingPoint),
            (Value::String("x".into()), ValueKind::String),
            (Value::Date("2026-09-02".into()), ValueKind::Date),
            (Value::Time("10:00:00".into()), ValueKind::Time),
            (
                Value::Timestamp("2026-09-02T10:00:00Z".into()),
                ValueKind::Timestamp,
            ),
            (Value::Uuid(Uuid::nil()), ValueKind::Uuid),
            (Value::Array(vec![]), ValueKind::Array),
            (Value::Object(vec![]), ValueKind::Object),
        ];
        for (value, kind) in cases {
            assert_eq!(value.kind(), kind, "{value:?}");
        }
    }

    #[test]
    fn a_decimal_keeps_its_exact_text() {
        let value = Value::Decimal(Decimal("19.99".into()));
        assert_eq!(value.render(20), "19.99");
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("19.99"));
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), value);
    }

    #[test]
    fn a_binary_preview_reports_the_whole_size_not_the_part_it_carries() {
        let preview = Binary::Preview {
            byte_count: 2_000_000_000,
            bytes: "AAAA".into(),
            media_type: Some("image/png".into()),
            digest: None,
        };
        assert_eq!(preview.byte_count(), 2_000_000_000);
        assert!(!preview.is_complete());
        assert_eq!(
            Value::Binary(preview).render(40),
            "2000000000 bytes (preview)"
        );
    }

    #[test]
    fn a_long_value_is_truncated_so_one_row_cannot_widen_the_grid() {
        let long = Value::String("x".repeat(500));
        let rendered = long.render(20);
        assert_eq!(rendered.chars().count(), 20);
        assert!(rendered.ends_with('…'), "the truncation is visible");
    }

    #[test]
    fn truncation_lands_on_a_character_boundary() {
        let rendered = truncate("ありがとうございます", 5);
        assert_eq!(rendered.chars().count(), 5);
        assert!(rendered.ends_with('…'));
    }

    #[test]
    fn a_value_that_fits_is_not_marked_as_truncated() {
        assert_eq!(truncate("short", 20), "short");
        assert_eq!(truncate("exactly-20-chars-abc", 20), "exactly-20-chars-abc");
    }

    #[test]
    fn json_numbers_keep_the_type_they_were_written_as() {
        assert_eq!(
            Value::from_json(&serde_json::json!(1)),
            Value::SignedInteger(1)
        );
        assert_eq!(
            Value::from_json(&serde_json::json!(1.5)),
            Value::FloatingPoint(1.5)
        );
        assert_eq!(
            Value::from_json(&serde_json::json!(-3)),
            Value::SignedInteger(-3)
        );
        assert_eq!(Value::from_json(&serde_json::json!(null)), Value::Null);
    }

    #[test]
    fn a_json_object_becomes_named_fields_in_order() {
        let value = Value::from_json(&serde_json::json!({"b": 2, "a": 1}));
        let Value::Object(fields) = value else {
            panic!("expected an object");
        };
        let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b"));
    }

    #[test]
    fn a_value_round_trips_through_json_including_nested_structure() {
        let value = Value::Object(vec![
            ObjectField {
                name: "id".into(),
                value: Value::Uuid(Uuid::nil()),
            },
            ObjectField {
                name: "tags".into(),
                value: Value::Array(vec![Value::String("a".into()), Value::Missing]),
            },
        ]);
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), value);
    }
}
