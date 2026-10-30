use yoke::{capture::Deadline, json};

#[test]
fn persisted_finite_float_values_roundtrip_exact_bits_and_canonical_bytes() {
    let mut numbers = vec![
        0.0,
        -0.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        f64::from_bits(1),
        -f64::from_bits(1),
        0.3229166666666667,
        1.0000000000000002,
    ];
    let mut state = 0x594f_4b45_5245_504fu64;
    for _ in 0..4096 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let number = f64::from_bits(state);
        if number.is_finite() {
            numbers.push(number);
        }
    }
    for number in numbers {
        let value = serde_json::json!({"weight":number,"label":"source \u{1f30d}\r\n"});
        let bytes = json::encode_bounded(&value, 256, &Deadline::new(5000)).unwrap();
        let parsed = json::parse(&bytes, 256).unwrap();
        assert_eq!(
            parsed["weight"].as_f64().unwrap().to_bits(),
            number.to_bits(),
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
    }
}

#[test]
fn exact_float_parsing_keeps_unique_keys_finite_values_and_protocol_budgets() {
    for input in [
        br#"{"weight":0.3229166666666667,"weight":1.0}"#.as_slice(),
        br#"{"weight":0.3229166666666667,"\u0077eight":1.0}"#,
        br#"{"weight":1e999}"#,
        br#"{"weight":NaN}"#,
        br#"{"weight":Infinity}"#,
        br#"{"weight":0.5} trailing"#,
    ] {
        assert!(json::parse(input, 256).is_err());
    }
    let input = br#"{"weight":0.3229166666666667}"#;
    assert!(json::parse(input, input.len() - 1).is_err());
    let deep = format!("{}0.3229166666666667{}", "[".repeat(66), "]".repeat(66));
    assert!(json::parse(deep.as_bytes(), 256).is_err());
    assert!(
        json::encode_bounded(
            &serde_json::json!({"weight":0.3229166666666667}),
            256,
            &Deadline::new(0)
        )
        .is_err()
    );
}
