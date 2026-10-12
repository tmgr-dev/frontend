use serde_json::{Number, Value};

/// JavaScript `Number(value)` on a SQLite value, as JSON (NaN becomes `null`, like `JSON.stringify`).
pub(crate) fn number(value: &Value) -> Value {
    match value {
        Value::Null => Value::from(0),
        Value::Bool(b) => Value::from(*b as i64),
        Value::Number(_) => value.clone(),
        Value::String(s) => float_json(parse_number(s)),
        _ => Value::Null,
    }
}

pub(crate) fn parse_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    t.parse::<f64>()
        .ok()
        .filter(|_| !t.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E'))
        .unwrap_or(f64::NAN)
}

pub(crate) fn float_json(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
        Value::from(f as i64)
    } else {
        Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// `Math.max(1, Number(raw ?? fallback) || fallback)` for paging parameters.
pub(crate) fn positive_or(raw: Option<&str>, fallback: i64) -> i64 {
    let n = raw.map_or(fallback as f64, parse_number);
    let n = if n.is_nan() || n == 0.0 {
        fallback as f64
    } else {
        n
    };
    n.max(1.0).trunc() as i64
}

/// JavaScript truthiness of a SQLite value.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// `String(value)` for a value that is a number or a string.
pub(crate) fn string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

/// `parseJson(raw, fallback)` from the TS serializer: bad or empty JSON yields the fallback.
pub(crate) fn parse_json(raw: &Value, fallback: Value) -> Value {
    match raw {
        Value::Null => fallback,
        Value::String(s) if s.is_empty() => fallback,
        Value::String(s) => serde_json::from_str(s).unwrap_or(fallback),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn number_follows_javascript() {
        assert_eq!(number(&json!(null)), json!(0));
        assert_eq!(number(&json!("12")), json!(12));
        assert_eq!(number(&json!(" 7 ")), json!(7));
        assert_eq!(number(&json!("")), json!(0));
        assert_eq!(number(&json!("abc")), json!(null));
        assert_eq!(number(&json!("2.5")), json!(2.5));
        assert_eq!(number(&json!("inf")), json!(null));
    }

    #[test]
    fn paging_parameters_fall_back_like_the_ts_api() {
        assert_eq!(positive_or(None, 20), 20);
        assert_eq!(positive_or(Some("0"), 20), 20);
        assert_eq!(positive_or(Some("x"), 20), 20);
        assert_eq!(positive_or(Some("-5"), 20), 1);
        assert_eq!(positive_or(Some("3"), 20), 3);
    }

    #[test]
    fn parse_json_uses_the_fallback() {
        assert_eq!(parse_json(&json!(null), json!([])), json!([]));
        assert_eq!(parse_json(&json!("{bad"), json!([])), json!([]));
        assert_eq!(parse_json(&json!("[1]"), json!([])), json!([1]));
    }
}
