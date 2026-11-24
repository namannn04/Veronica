use serde::Serialize;
use serde_json::{Map, Value};

const INDENT: &str = "  ";
const HEX_DIGITS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

pub fn to_string<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<String> {
    Ok(value_to_string(&serde_json::to_value(value)?))
}

pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<String> {
    Ok(value_to_string_pretty(&serde_json::to_value(value)?))
}

pub fn value_to_string(value: &Value) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

pub fn value_to_string_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_pretty(&mut out, value, 0);
    out.push('\n');
    out
}

fn sorted(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    entries
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c < ' ' => {
                let code = u32::from(c);
                out.push_str("\\u00");
                out.push(HEX_DIGITS[(code >> 4) as usize]);
                out.push(HEX_DIGITS[(code & 0xf) as usize]);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_compact(out: &mut String, value: &Value) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (index, (key, item)) in sorted(map).into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_compact(out, item);
            }
            out.push('}');
        }
        Value::String(text) => write_string(out, text),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
    }
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    for _ in 0..depth {
        out.push_str(INDENT);
    }
}

fn write_pretty(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Array(items) if !items.is_empty() => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_pretty(out, item, depth + 1);
            }
            newline(out, depth);
            out.push(']');
        }
        Value::Object(map) if !map.is_empty() => {
            out.push('{');
            for (index, (key, item)) in sorted(map).into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_string(out, key);
                out.push_str(": ");
                write_pretty(out, item, depth + 1);
            }
            newline(out, depth);
            out.push('}');
        }
        other => write_compact(out, other),
    }
}
