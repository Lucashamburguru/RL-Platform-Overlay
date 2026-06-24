use serde_json::Value;

pub enum DecodedValue<'a> {
    Borrowed(&'a Value),
    Owned(Value),
}

impl<'a> DecodedValue<'a> {
    pub fn into_owned(self) -> Value {
        match self {
            Self::Borrowed(v) => v.clone(),
            Self::Owned(v) => v,
        }
    }
}

impl<'a> std::ops::Deref for DecodedValue<'a> {
    type Target = Value;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(v) => v,
            Self::Owned(v) => v,
        }
    }
}

pub fn decode_json_string_value(value: &Value) -> DecodedValue<'_> {
    if let Some(encoded) = value.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(encoded) {
            DecodedValue::Owned(parsed)
        } else {
            DecodedValue::Borrowed(value)
        }
    } else {
        DecodedValue::Borrowed(value)
    }
}

pub fn string_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value[*key].as_str())
}

pub fn number_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value[*key]
            .as_i64()
            .or_else(|| value[*key].as_u64().map(|v| v as i64))
            .or_else(|| value[*key].as_str()?.parse().ok())
    })
}

pub fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| value[*key].as_bool())
}
