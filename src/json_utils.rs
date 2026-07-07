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
    keys.iter().find_map(|key| value.get(*key)?.as_str())
}

pub fn number_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        let v = value.get(*key)?;
        v.as_i64()
            .or_else(|| v.as_u64().and_then(|x| i64::try_from(x).ok()))
            .or_else(|| v.as_str()?.parse().ok())
    })
}

pub fn number_field_u8(value: &Value, keys: &[&str]) -> Option<u8> {
    number_field(value, keys).and_then(checked_u8)
}

pub fn number_field_u32(value: &Value, keys: &[&str]) -> Option<u32> {
    number_field(value, keys).and_then(checked_u32)
}

pub fn number_field_i32(value: &Value, keys: &[&str]) -> Option<i32> {
    number_field(value, keys).and_then(checked_i32)
}

pub fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| value.get(*key)?.as_bool())
}

pub fn checked_u8(value: i64) -> Option<u8> {
    u8::try_from(value).ok()
}

pub fn checked_u32(value: i64) -> Option<u32> {
    u32::try_from(value).ok()
}

pub fn checked_i32(value: i64) -> Option<i32> {
    i32::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_integer_conversions_reject_negative_and_out_of_range_values() {
        assert_eq!(checked_u8(0), Some(0));
        assert_eq!(checked_u8(255), Some(255));
        assert_eq!(checked_u8(-1), None);
        assert_eq!(checked_u8(256), None);

        assert_eq!(checked_u32(0), Some(0));
        assert_eq!(checked_u32(u32::MAX as i64), Some(u32::MAX));
        assert_eq!(checked_u32(-1), None);
        assert_eq!(checked_u32(u32::MAX as i64 + 1), None);

        assert_eq!(checked_i32(0), Some(0));
        assert_eq!(checked_i32(i32::MIN as i64), Some(i32::MIN));
        assert_eq!(checked_i32(i32::MAX as i64), Some(i32::MAX));
        assert_eq!(checked_i32(i32::MIN as i64 - 1), None);
        assert_eq!(checked_i32(i32::MAX as i64 + 1), None);
    }

    #[test]
    fn number_field_rejects_unsigned_values_that_do_not_fit_i64() {
        let value = serde_json::json!({
            "fits": i64::MAX as u64,
            "too_large": i64::MAX as u64 + 1,
            "fallback": "7"
        });

        assert_eq!(number_field(&value, &["fits"]), Some(i64::MAX));
        assert_eq!(number_field(&value, &["too_large"]), None);
        assert_eq!(number_field(&value, &["missing", "fallback"]), Some(7));
    }

    #[test]
    fn typed_number_fields_apply_checked_conversions() {
        let value = serde_json::json!({
            "small": 255,
            "too_big_u8": 256,
            "u32_max": u32::MAX as u64,
            "too_big_u32": u32::MAX as u64 + 1,
            "i32_min": i32::MIN,
            "too_small_i32": i32::MIN as i64 - 1
        });

        assert_eq!(number_field_u8(&value, &["small"]), Some(255));
        assert_eq!(number_field_u8(&value, &["too_big_u8"]), None);
        assert_eq!(number_field_u32(&value, &["u32_max"]), Some(u32::MAX));
        assert_eq!(number_field_u32(&value, &["too_big_u32"]), None);
        assert_eq!(number_field_i32(&value, &["i32_min"]), Some(i32::MIN));
        assert_eq!(number_field_i32(&value, &["too_small_i32"]), None);
    }
}
