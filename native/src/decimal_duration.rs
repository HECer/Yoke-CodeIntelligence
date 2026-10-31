//! Integral policy durations from the canonical decimal of an admitted number.
use serde_json::Value;

/// No binary floating-point multiplication is performed. This uses the number's
/// canonical decimal representation, not an assertion about original YAML text.
/// Zero policy (unlimited versus invalid) remains the caller's responsibility.
pub(crate) fn milliseconds_ceil(value: &Value, maximum: u64) -> Option<u64> {
    let text = value.as_number()?.to_string();
    if text.len() > 64 {
        return None;
    }
    let negative = text.starts_with('-');
    let unsigned = text.strip_prefix('-').unwrap_or(&text);
    let (mantissa, exponent) = unsigned
        .split_once('e')
        .or_else(|| unsigned.split_once('E'))
        .unwrap_or((unsigned, "0"));
    let exponent = exponent.parse::<i32>().ok()?;
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let significand = whole
        .bytes()
        .chain(fraction.bytes())
        .try_fold(0_u128, |n, b| {
            let digit = b.checked_sub(b'0').filter(|d| *d <= 9)?;
            n.checked_mul(10)?.checked_add(u128::from(digit))
        })?;
    if negative && significand != 0 {
        return None;
    }
    let scale = exponent
        .checked_add(3)?
        .checked_sub(i32::try_from(fraction.len()).ok()?)?;
    let rounded = if significand == 0 {
        0
    } else if scale >= 0 {
        significand.checked_mul(10_u128.checked_pow(u32::try_from(scale).ok()?)?)?
    } else if let Some(divisor) = 10_u128.checked_pow(scale.unsigned_abs()) {
        (significand / divisor).checked_add(u128::from(significand % divisor != 0))?
    } else {
        // A positive significand smaller than 10^39 is below one millisecond.
        1
    };
    u64::try_from(rounded).ok().filter(|ms| *ms <= maximum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_millisecond_grid_does_not_acquire_binary_float_rounding_error() {
        for millis in 1..=20_000_u64 {
            let text = format!("{}.{:03}", millis / 1000, millis % 1000);
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(milliseconds_ceil(&value, 600_000), Some(millis), "{text}");
        }
    }

    #[test]
    fn real_fractional_milliseconds_round_up_including_positive_subnormals() {
        for (text, expected) in [
            ("2.007", 2007),
            ("2.007000000000001", 2008),
            ("2.0069999999999997", 2007),
            ("1.125", 1125),
            ("1e-5", 1),
            ("5e-324", 1),
            ("5.99999e2", 600000),
            ("600", 600000),
        ] {
            let value: Value = serde_json::from_str(text).unwrap();
            assert_eq!(milliseconds_ceil(&value, 600_000), Some(expected), "{text}");
        }
        assert_eq!(
            milliseconds_ceil(&json!(f64::from_bits(1)), 600000),
            Some(1)
        );
    }

    #[test]
    fn zeros_types_negative_values_and_overflow_retain_closed_bounds() {
        for value in [json!(0), json!(-0.0)] {
            assert_eq!(milliseconds_ceil(&value, 600_000), Some(0));
        }
        for value in [
            Value::Null,
            json!(true),
            json!("2.007"),
            json!([]),
            json!({}),
            json!(-0.001),
            json!(-1),
            json!(600.0001),
            json!(u64::MAX),
            json!(1e308),
        ] {
            assert_eq!(milliseconds_ceil(&value, 600_000), None, "{value}");
        }
        assert_eq!(milliseconds_ceil(&json!(0.001), 0), None);
    }
}
