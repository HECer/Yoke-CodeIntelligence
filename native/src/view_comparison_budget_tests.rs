//! Private owned serializer/counter oracles; no source/provider/Store fixtures.
use super::*;
use crate::capture::DeadlineFailure;
use serde::ser::{Error as _, SerializeSeq};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

struct Poison(Arc<AtomicUsize>);
impl Serialize for Poison {
    fn serialize<S: Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(S::Error::custom("owned_poison_serializer_was_visited"))
    }
}
struct ProgressThenPoison {
    progress: Arc<AtomicUsize>,
    poison: Arc<AtomicUsize>,
    cancel: Option<Arc<AtomicBool>>,
}
impl Serialize for ProgressThenPoison {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(2))?;
        sequence.serialize_element("abc")?;
        self.progress.fetch_add(1, Ordering::SeqCst);
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        // The comma write must stop before Serialize is invoked for this value.
        sequence.serialize_element(&Poison(self.poison.clone()))?;
        sequence.end()
    }
}
fn counter(deadline: &Deadline, maximum: usize) -> ByteSink<'_> {
    ByteSink {
        deadline,
        maximum,
        count: 0,
        bytes: None,
        hash: None,
        failure: None,
    }
}

#[test]
fn bounded_serializer_exact_bytes_and_next_byte_refuse_before_poison() {
    let deadline = Deadline::new(30_000);
    let value = json!(["abc", "z"]);
    let bytes = b"[\"abc\",\"z\"]";
    assert_eq!(
        encode_bounded(&value, bytes.len(), &deadline).unwrap(),
        bytes
    );
    assert!(
        encode_bounded(&value, bytes.len() - 1, &deadline)
            .unwrap_err()
            .to_string()
            .contains("comparison_capacity_exceeded")
    );
    let progress = Arc::new(AtomicUsize::new(0));
    let poison = Arc::new(AtomicUsize::new(0));
    let owned = ProgressThenPoison {
        progress: progress.clone(),
        poison: poison.clone(),
        cancel: None,
    };
    let exact_prefix = b"[\"abc\"".len();
    let mut sink = counter(&deadline, exact_prefix);
    let error = serialize_into(&owned, &mut sink).unwrap_err();
    assert_eq!(
        sink.count, exact_prefix,
        "Actual first element reached the writer"
    );
    assert_eq!(progress.load(Ordering::SeqCst), 1);
    assert_eq!(poison.load(Ordering::SeqCst), 0);
    assert!(error.to_string().contains("comparison_capacity_exceeded"));
    assert_eq!(deadline.failure(), None);
}

#[test]
fn canonical_value_conversion_is_preflighted_before_poison_serialize() {
    let deadline = Deadline::new(30_000);
    let progress = Arc::new(AtomicUsize::new(0));
    let poison = Arc::new(AtomicUsize::new(0));
    let owned = ProgressThenPoison {
        progress: progress.clone(),
        poison: poison.clone(),
        cancel: None,
    };
    let error = canonical_digest_limited(&owned, b"[\"abc\"".len(), &deadline).unwrap_err();
    assert_eq!(progress.load(Ordering::SeqCst), 1);
    assert_eq!(
        poison.load(Ordering::SeqCst),
        0,
        "Do not materialize the complete Value before bounded admission"
    );
    assert!(error.to_string().contains("comparison_capacity_exceeded"));
}

#[test]
fn bounded_typed_digest_preserves_released_serialization_order() {
    #[derive(Serialize)]
    struct Original<'a> {
        z: &'a str,
        a: &'a str,
    }
    let deadline = Deadline::new(30_000);
    let original = Original {
        z: "last",
        a: "first",
    };
    let literal = br#"{"z":"last","a":"first"}"#;
    let bounded = serialized_digest(&original, literal.len(), &deadline).unwrap();
    assert_eq!(bounded, crate::model::identity(&original));
    assert_eq!(bounded, bytes_digest(literal, &deadline).unwrap());
    assert_ne!(bounded, canonical_digest(&original, &deadline).unwrap());
    assert!(
        serialized_digest(&original, literal.len() - 1, &deadline)
            .unwrap_err()
            .to_string()
            .contains("comparison_capacity_exceeded")
    );
}

#[test]
fn serializer_cancellation_after_actual_progress_uses_same_inherited_deadline() {
    let parent = Deadline::new(30_000);
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicUsize::new(0));
    let poison = Arc::new(AtomicUsize::new(0));
    let outer = Deadline::run_bounded(&parent, cancel.clone(), || {
        let inherited = Deadline::new(30_000);
        assert_eq!(inherited.failure(), None);
        assert_eq!(
            encode_bounded(&json!(["abc"]), 7, &inherited).unwrap(),
            b"[\"abc\"]"
        );
        let owned = ProgressThenPoison {
            progress: progress.clone(),
            poison: poison.clone(),
            cancel: Some(cancel.clone()),
        };
        let mut sink = counter(&inherited, 1_000);
        let error = serialize_into(&owned, &mut sink).unwrap_err();
        assert_eq!(sink.count, b"[\"abc\"".len());
        assert_eq!(progress.load(Ordering::SeqCst), 1);
        assert_eq!(poison.load(Ordering::SeqCst), 0);
        assert_eq!(inherited.failure(), Some(DeadlineFailure::Cancelled));
        assert_eq!(error.to_string(), "Request cancelled by host");
        assert!(cancel.load(Ordering::SeqCst));
        Ok(())
    });
    assert_eq!(outer.unwrap_err().to_string(), "Request cancelled by host");
    assert!(
        cancel.load(Ordering::SeqCst),
        "Cancellation is never reset to pass the outer postcheck"
    );
}

struct RowProbe {
    progress: usize,
    trigger: usize,
    cancel: Arc<AtomicBool>,
}
thread_local! { static ROW_PROBE:RefCell<Option<RowProbe>>=const { RefCell::new(None) }; }
pub(super) fn row_counter_progress() {
    ROW_PROBE.with(|slot| {
        if let Some(probe) = slot.borrow_mut().as_mut() {
            probe.progress += 1;
            if probe.progress == probe.trigger {
                probe.cancel.store(true, Ordering::SeqCst);
            }
        }
    });
}
struct ClearRowProbe;
impl Drop for ClearRowProbe {
    fn drop(&mut self) {
        ROW_PROBE.with(|slot| {
            slot.replace(None);
        });
    }
}

#[test]
fn recursive_rows_count_all_copies_and_cancel_only_after_completed_child_progress() {
    let parent = Deadline::new(30_000);
    let value = json!({"header":[1,2],"nested":[{"ids":[1,1]},[null]]});
    assert_eq!(row_units(&value, &parent).unwrap(), 7);
    check_component_rows(&value, 7, &parent).unwrap();
    assert!(
        check_component_rows(&value, 6, &parent)
            .unwrap_err()
            .to_string()
            .contains("comparison_capacity_exceeded")
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let outer = Deadline::run_bounded(&parent, cancel.clone(), || {
        let inherited = Deadline::new(30_000);
        ROW_PROBE.with(|slot| {
            assert!(
                slot.replace(Some(RowProbe {
                    progress: 0,
                    trigger: 1,
                    cancel: cancel.clone()
                }))
                .is_none()
            );
        });
        let _clear = ClearRowProbe;
        let error = row_units(&json!([1, 2, 3]), &inherited).unwrap_err();
        ROW_PROBE.with(|slot| {
            assert_eq!(slot.borrow().as_ref().unwrap().progress, 1);
        });
        assert_eq!(inherited.failure(), Some(DeadlineFailure::Cancelled));
        assert_eq!(error.to_string(), "Request cancelled by host");
        assert!(cancel.load(Ordering::SeqCst));
        Ok(())
    });
    assert_eq!(outer.unwrap_err().to_string(), "Request cancelled by host");
    assert!(cancel.load(Ordering::SeqCst));
}

#[test]
fn recursive_rows_check_same_deadline_after_final_child_progress() {
    let parent = Deadline::new(30_000);
    let cancel = Arc::new(AtomicBool::new(false));
    let outer = Deadline::run_bounded(&parent, cancel.clone(), || {
        let inherited = Deadline::new(30_000);
        assert_eq!(row_units(&json!([1]), &inherited).unwrap(), 1);
        ROW_PROBE.with(|slot| {
            assert!(
                slot.replace(Some(RowProbe {
                    progress: 0,
                    trigger: 1,
                    cancel: cancel.clone(),
                }))
                .is_none()
            );
        });
        let _clear = ClearRowProbe;
        let error = row_units(&json!([1]), &inherited).unwrap_err();
        ROW_PROBE.with(|slot| assert_eq!(slot.borrow().as_ref().unwrap().progress, 1));
        assert_eq!(inherited.failure(), Some(DeadlineFailure::Cancelled));
        assert_eq!(error.to_string(), "Request cancelled by host");
        assert!(cancel.load(Ordering::SeqCst));
        Ok(())
    });
    assert_eq!(outer.unwrap_err().to_string(), "Request cancelled by host");
    assert!(cancel.load(Ordering::SeqCst));
}
