use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};
use veronica_protocol::canonical;

#[test]
fn compact_sorts_keys_without_whitespace() {
    let value = json!({ "zeta": 1, "alpha": true, "Mid": null, "mid": "x" });
    assert_eq!(
        canonical::value_to_string(&value),
        r#"{"Mid":null,"alpha":true,"mid":"x","zeta":1}"#
    );
}

#[test]
fn nested_objects_are_sorted_and_arrays_keep_order() {
    let value = json!({
        "b": [{ "y": 2, "x": 1 }, 3, [{ "d": 4, "c": 3 }]],
        "a": { "q": { "s": 1, "r": 2 }, "p": [] }
    });
    assert_eq!(
        canonical::value_to_string(&value),
        r#"{"a":{"p":[],"q":{"r":2,"s":1}},"b":[{"x":1,"y":2},3,[{"c":3,"d":4}]]}"#
    );
}

#[test]
fn pretty_indents_two_spaces_and_ends_with_a_newline() {
    let value = json!({ "b": [1, { "d": {}, "c": [] }], "a": "x" });
    let expected = "{\n  \"a\": \"x\",\n  \"b\": [\n    1,\n    {\n      \"c\": [],\n      \"d\": {}\n    }\n  ]\n}\n";
    assert_eq!(canonical::value_to_string_pretty(&value), expected);
    assert_eq!(canonical::value_to_string_pretty(&json!(7)), "7\n");
    assert!(!canonical::value_to_string(&value).ends_with('\n'));
}

#[test]
fn struct_field_order_does_not_leak_into_output() {
    #[derive(Serialize)]
    struct Report {
        zebra: u8,
        apple: u8,
        mango: HashMap<String, u8>,
    }
    let mango = HashMap::from([
        ("z".to_string(), 1),
        ("a".to_string(), 2),
        ("m".to_string(), 3),
    ]);
    let text = canonical::to_string(&Report {
        zebra: 1,
        apple: 2,
        mango,
    })
    .unwrap();
    assert_eq!(text, r#"{"apple":2,"mango":{"a":2,"m":3,"z":1},"zebra":1}"#);
}

#[test]
fn unicode_passes_through_and_control_characters_are_escaped() {
    let value =
        json!({ "ünïcode": "héllo 🎵 \u{2028}", "ctl": "a\"b\\c\n\t\r\u{08}\u{0c}\u{01}\u{1f}" });
    let text = canonical::value_to_string(&value);
    assert_eq!(
        text,
        "{\"ctl\":\"a\\\"b\\\\c\\n\\t\\r\\b\\f\\u0001\\u001f\",\"ünïcode\":\"héllo 🎵 \u{2028}\"}"
    );
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), value);
}

#[test]
fn keys_sort_by_code_point() {
    let value = json!({ "é": 1, "z": 2, "Z": 3, "10": 4, "9": 5 });
    assert_eq!(
        canonical::value_to_string(&value),
        r#"{"10":4,"9":5,"Z":3,"z":2,"é":1}"#
    );
}

#[test]
fn numbers_keep_their_serde_json_form() {
    let value = json!({ "int": -3, "big": u64::MAX, "float": 1.5, "whole": 2.0 });
    assert_eq!(
        canonical::value_to_string(&value),
        r#"{"big":18446744073709551615,"float":1.5,"int":-3,"whole":2.0}"#
    );
}

#[test]
fn non_finite_floats_become_null_in_both_forms() {
    #[derive(Serialize)]
    struct Sample {
        nan: f64,
        up: f64,
        down: f32,
        fine: f64,
    }
    let sample = Sample {
        nan: f64::NAN,
        up: f64::INFINITY,
        down: f32::NEG_INFINITY,
        fine: 0.25,
    };
    assert_eq!(
        canonical::to_string(&sample).unwrap(),
        r#"{"down":null,"fine":0.25,"nan":null,"up":null}"#
    );
    assert_eq!(
        canonical::to_string_pretty(&sample).unwrap(),
        "{\n  \"down\": null,\n  \"fine\": 0.25,\n  \"nan\": null,\n  \"up\": null\n}\n"
    );
    assert_eq!(canonical::to_string(&[f64::NAN]).unwrap(), "[null]");
}

#[test]
fn output_is_valid_json_equal_to_the_input() {
    let value: Value = serde_json::from_str(include_str!("fixtures/media.json")).unwrap();
    for text in [
        canonical::value_to_string(&value),
        canonical::value_to_string_pretty(&value),
    ] {
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), value);
    }
}
