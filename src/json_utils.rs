use serde_json::Value;

pub fn decode_json_string_value(value: &Value) -> Value {
    if let Some(encoded) = value.as_str() {
        serde_json::from_str::<Value>(encoded).unwrap_or_else(|_| value.clone())
    } else {
        value.clone()
    }
}

pub fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value[*key].as_str())
}

pub fn number_field(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value[*key]
            .as_u64()
            .or_else(|| value[*key].as_str()?.parse().ok())
    })
}

pub fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| value[*key].as_bool())
}
