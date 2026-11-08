use std::io::{Cursor, Read};
use yoke::{
    capture::Deadline,
    falkor_wire::{self as wire, Limits, Value},
};
#[test]
fn command_bulk_lengths_keep_unicode_nuls_and_crlf_as_data() {
    let d = Deadline::new(30000);
    let text = "owned \u{1f4a1}\0\r\n*2\r\n$4\r\nAUTH\r\n";
    let args = [b"GRAPH.QUERY".as_slice(), b"owned:key", text.as_bytes()];
    let bytes = wire::encode(&args, &Default::default(), &d).unwrap();
    assert_eq!(
        wire::decode(&bytes, &Default::default(), &d).unwrap(),
        Value::Array(Some(
            args.iter().map(|a| Value::Bulk(Some(a.to_vec()))).collect()
        ))
    );
    assert!(
        !format!(
            "{:?}",
            wire::encode(
                &[b"secret"],
                &Limits {
                    max_bytes: 1,
                    ..Default::default()
                },
                &d
            )
            .err()
        )
        .contains("secret")
    );
}
#[test]
fn nil_empty_error_binary_and_nested_replies_remain_distinct() {
    let raw=b"*6\r\n$-1\r\n$0\r\n\r\n*-1\r\n*0\r\n-ERR untrusted\r\n*2\r\n:-9223372036854775808\r\n$2\r\n\xff\0\r\n";
    assert_eq!(
        wire::decode(raw, &Default::default(), &Deadline::new(30000)).unwrap(),
        Value::Array(Some(vec![
            Value::Bulk(None),
            Value::Bulk(Some(vec![])),
            Value::Array(None),
            Value::Array(Some(vec![])),
            Value::Error(b"ERR untrusted".to_vec()),
            Value::Array(Some(vec![
                Value::Integer(i64::MIN),
                Value::Bulk(Some(vec![255, 0]))
            ]))
        ]))
    );
}
#[test]
fn malformed_numbers_framing_truncation_and_trailing_replies_are_refused() {
    for raw in [
        b":01\r\n".as_slice(),
        b":+1\r\n",
        b":-0\r\n",
        b":9223372036854775808\r\n",
        b"$-2\r\n",
        b"*-2\r\n",
        b"+ok\n",
        b"+ok\rx",
        b"$2\r\na\r\n",
        b"$1\r\naXX",
        b"+ok\r\n+extra\r\n",
        b",1.0\r\n",
        b"%0\r\n",
    ] {
        assert!(wire::decode(raw, &Default::default(), &Deadline::new(30000)).is_err());
    }
}
#[test]
fn exact_capacity_is_admitted_and_lower_budgets_refuse_before_bulk_allocation() {
    let d = Deadline::new(30000);
    let bytes = wire::encode(&[b"one"], &Default::default(), &d).unwrap();
    let mut limits = Limits {
        max_bytes: bytes.len(),
        ..Default::default()
    };
    assert!(wire::decode(&bytes, &limits, &d).is_ok());
    assert_eq!(
        wire::encode(&[b"one"], &limits, &d).unwrap().as_slice(),
        bytes.as_slice()
    );
    limits.max_bytes -= 1;
    assert!(wire::decode(&bytes, &limits, &d).is_err());
    assert!(wire::encode(&[b"one"], &limits, &d).is_err());
    limits = Limits {
        max_bulk_bytes: 2,
        ..Default::default()
    };
    assert!(wire::encode(&[b"one"], &limits, &d).is_err());
    assert!(wire::decode(b"$3\r\n", &limits, &d).is_err());
    assert!(serde_json::from_str::<Limits>(r#"{"allowNetwork":true}"#).is_err());
    assert!(
        Limits {
            max_bytes: 32_000_001,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}
#[test]
fn array_counts_and_nesting_have_independent_admission_limits() {
    let d = Deadline::new(30000);
    assert!(wire::decode(b"*100001\r\n", &Default::default(), &d).is_err());
    let limits = Limits {
        max_values: 3,
        ..Default::default()
    };
    assert!(wire::decode(b"*2\r\n*1\r\n:1\r\n:2\r\n", &limits, &d).is_err());
    let limits = Limits {
        max_depth: 0,
        ..Default::default()
    };
    assert!(wire::decode(b":1\r\n", &limits, &d).is_ok());
    assert!(wire::decode(b"*1\r\n:1\r\n", &limits, &d).is_err());
}
#[test]
fn expiry_does_no_io_and_short_reads_keep_exact_frame_boundaries() {
    struct Reader {
        cursor: Cursor<Vec<u8>>,
        reads: usize,
    }
    impl Read for Reader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.reads += 1;
            let n = out.len().min(1);
            self.cursor.read(&mut out[..n])
        }
    }
    let mut reader = Reader {
        cursor: Cursor::new(b"+ok\r\n:7\r\n".to_vec()),
        reads: 0,
    };
    assert!(wire::read(&mut reader, &Default::default(), &Deadline::new(0)).is_err());
    assert_eq!(reader.reads, 0);
    assert_eq!(
        wire::read(&mut reader, &Default::default(), &Deadline::new(30000)).unwrap(),
        Value::Simple(b"ok".to_vec())
    );
    assert_eq!(reader.cursor.position(), 5);
    assert_eq!(
        wire::read(&mut reader, &Default::default(), &Deadline::new(30000)).unwrap(),
        Value::Integer(7)
    );
}
