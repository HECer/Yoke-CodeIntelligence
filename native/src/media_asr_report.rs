//! Validate original SDK observations and separately qualify their audio overlap.
use crate::{capture::Deadline, model::identity, paths::obvious_secret};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Raw {
    pub schema: u32,
    pub job_id: String,
    pub library_version: String,
    pub audio_sha256: String,
    pub audio_frames: u64,
    pub sample_rate: u32,
    pub language: String,
    pub duration_seconds: f64,
    pub requested_compute_type: String,
    pub segments: Vec<Segment>,
}

pub(super) struct Expected<'a> {
    pub job_id: &'a str,
    pub source_hash: &'a str,
    pub frames: u64,
    pub data_offset: u64,
    pub library_version: &'a str,
    pub language: &'a str,
    pub max_segments: usize,
    pub max_text_bytes: usize,
}

pub(super) struct Checked {
    pub raw: Raw,
    pub segments: Vec<Value>,
    pub text_bytes: usize,
    pub uncertain_temporal_segments: usize,
}

pub(super) fn check(bytes: &[u8], expected: &Expected<'_>, d: &Deadline) -> Result<Checked> {
    d.check()?;
    let raw: Raw = (|| crate::json::decode(crate::json::parse(bytes, 2_000_000)?))()
        .map_err(|_: anyhow::Error| anyhow::anyhow!("ASR response protocol invalid"))?;
    let duration = expected.frames as f64 / 16_000.0;
    ensure!(
        raw.schema == 1
            && raw.job_id == expected.job_id
            && raw.audio_sha256 == expected.source_hash
            && raw.audio_frames == expected.frames
            && raw.sample_rate == 16_000
            && raw.library_version == expected.library_version
            && raw.language == expected.language
            && raw.requested_compute_type == "int8"
            && raw.duration_seconds.is_finite()
            && (raw.duration_seconds - duration).abs() <= 0.000_001,
        "ASR response binding mismatch"
    );
    ensure!(
        raw.segments.len() <= expected.max_segments,
        "ASR segment capacity exceeded"
    );
    let mut segments = Vec::with_capacity(raw.segments.len());
    let mut text_bytes = 0usize;
    let mut uncertain_temporal_segments = 0usize;
    for (ordinal, segment) in raw.segments.iter().enumerate() {
        d.check()?;
        ensure!(
            segment.start.is_finite()
                && segment.end.is_finite()
                && segment.start >= 0.0
                && segment.start <= segment.end
                && segment.end <= 3600.0,
            "ASR nominal timestamp invalid"
        );
        text_bytes = text_bytes
            .checked_add(segment.text.len())
            .ok_or_else(|| anyhow::anyhow!("ASR transcript capacity exceeded"))?;
        ensure!(
            !segment.text.trim().is_empty()
                && text_bytes <= expected.max_text_bytes
                && !obvious_secret(&segment.text)
                && !segment
                    .text
                    .chars()
                    .any(|c| { c.is_control() && !matches!(c, '\r' | '\n' | '\t') }),
            "ASR transcript invalid, secret or over capacity"
        );
        let start = if segment.start >= duration {
            expected.frames
        } else {
            ((segment.start * 16_000.0).floor() as u64).min(expected.frames)
        };
        let end = if segment.end >= duration {
            expected.frames
        } else {
            ((segment.end * 16_000.0).ceil() as u64).min(expected.frames)
        };
        let positive_overlap = segment.start < segment.end && start < end;
        let classification = if !positive_overlap {
            "no_positive_sample_overlap"
        } else if segment.end > duration {
            "partial_overlap_with_captured_audio"
        } else {
            "nominal_interval_within_captured_audio"
        };
        if classification != "nominal_interval_within_captured_audio" {
            uncertain_temporal_segments += 1;
        }
        let overlap = if positive_overlap {
            let first = expected
                .data_offset
                .checked_add(
                    start
                        .checked_mul(2)
                        .ok_or_else(|| anyhow::anyhow!("ASR sample offset overflow"))?,
                )
                .ok_or_else(|| anyhow::anyhow!("ASR sample offset overflow"))?;
            let last = expected
                .data_offset
                .checked_add(
                    end.checked_mul(2)
                        .ok_or_else(|| anyhow::anyhow!("ASR sample offset overflow"))?,
                )
                .ok_or_else(|| anyhow::anyhow!("ASR sample offset overflow"))?;
            json!({"startFrame":start,"endFrame":end,"startByte":first,"endByte":last,"sourceHash":expected.source_hash,"basis":"nominal_interval_intersection_pcm16_mono_v1","semanticAlignmentEstablished":false,"literalTextQuote":false})
        } else {
            Value::Null
        };
        segments.push(json!({"id":identity(&("native_asr_segment_v1",expected.job_id,ordinal)),"ordinal":ordinal,"text":segment.text,"reportedStartSeconds":segment.start,"reportedEndSeconds":segment.end,"temporalClassification":classification,"capturedSampleEnvelope":overlap,"exactSpeechAlignmentEstablished":false}));
    }
    d.check()?;
    Ok(Checked {
        raw,
        segments,
        text_bytes,
        uncertain_temporal_segments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Expected<'static> {
        Expected {
            job_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            source_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            frames: 99_040,
            data_offset: 46,
            library_version: "1.2.1",
            language: "en",
            max_segments: 512,
            max_text_bytes: 256_000,
        }
    }
    fn report(segments: Value) -> Value {
        let e = context();
        json!({"schema":1,"jobId":e.job_id,"libraryVersion":e.library_version,"audioSha256":e.source_hash,"audioFrames":e.frames,"sampleRate":16000,"language":"en","durationSeconds":6.19,"requestedComputeType":"int8","segments":segments})
    }
    #[test]
    fn actual_sdk_padding_and_parallel_observations_keep_original_times_and_separate_sample_envelopes()
     {
        let data = report(json!([
            {"start":0.0,"end":3.92,"text":" Native audio evidence stays separate from inferred words."},
            {"start":3.92,"end":7.0,"text":" This is an owned local test."},
            {"start":3.92,"end":7.0,"text":" This is an owned local test."}
        ]));
        let checked = check(
            &serde_json::to_vec(&data).unwrap(),
            &context(),
            &Deadline::new(30_000),
        )
        .unwrap();
        assert_eq!(checked.raw.segments[1].end, 7.0);
        assert_eq!(checked.segments.len(), 3);
        assert_ne!(checked.segments[1]["id"], checked.segments[2]["id"]);
        assert_eq!(checked.segments[1]["reportedEndSeconds"], 7.0);
        assert_eq!(
            checked.segments[1]["capturedSampleEnvelope"]["endFrame"],
            99_040
        );
        assert_eq!(
            checked.segments[1]["capturedSampleEnvelope"]["endByte"],
            198_126
        );
        assert_eq!(
            checked.segments[1]["temporalClassification"],
            "partial_overlap_with_captured_audio"
        );
        assert_eq!(checked.uncertain_temporal_segments, 2);
        assert_eq!(
            checked.segments[1]["capturedSampleEnvelope"]["literalTextQuote"],
            false
        );
    }

    #[test]
    fn missing_overlap_is_explicit_and_true_zero_utf8_budgets_never_clip_output() {
        let data = report(json!([{"start":8.0,"end":9.0,"text":"e\u{301}\u{1f600}"}]));
        let bytes = serde_json::to_vec(&data).unwrap();
        let mut e = context();
        e.max_text_bytes = 7;
        let checked = check(&bytes, &e, &Deadline::new(30_000)).unwrap();
        assert_eq!(checked.text_bytes, 7);
        assert!(checked.segments[0]["capturedSampleEnvelope"].is_null());
        assert_eq!(
            checked.segments[0]["temporalClassification"],
            "no_positive_sample_overlap"
        );
        e.max_text_bytes = 6;
        assert!(check(&bytes, &e, &Deadline::new(30_000)).is_err());
        e.max_segments = 0;
        assert!(check(&bytes, &e, &Deadline::new(30_000)).is_err());
        e.max_text_bytes = 0;
        assert!(
            check(
                &serde_json::to_vec(&report(json!([]))).unwrap(),
                &e,
                &Deadline::new(30_000)
            )
            .is_ok()
        );
    }

    #[test]
    fn points_and_wholly_outside_intervals_never_invent_positive_sample_evidence() {
        for (start, end) in [(0.000_02, 0.000_02), (8.0, 9.0), (6.19, 7.0)] {
            let data = report(json!([{"start":start,"end":end,"text":"nominal"}]));
            let checked = check(
                &serde_json::to_vec(&data).unwrap(),
                &context(),
                &Deadline::new(30_000),
            )
            .unwrap();
            assert!(checked.segments[0]["capturedSampleEnvelope"].is_null());
            assert_eq!(
                checked.segments[0]["temporalClassification"],
                "no_positive_sample_overlap"
            );
        }
    }

    #[test]
    fn forged_binding_fields_extra_authority_and_invalid_nominal_ranges_are_refused() {
        let good = report(json!([{"start":0.0,"end":1.0,"text":"original"}]));
        for (name, value) in [
            ("jobId", json!("c".repeat(64))),
            ("audioSha256", json!("d".repeat(64))),
            ("audioFrames", json!(99_039)),
            ("sampleRate", json!(8_000)),
            ("durationSeconds", json!(7.0)),
            ("libraryVersion", json!("other")),
            ("language", json!("de")),
            ("requestedComputeType", json!("cuda")),
            ("sourceExecutionAllowed", json!(true)),
        ] {
            let mut forged = good.clone();
            forged[name] = value;
            assert!(
                check(
                    &serde_json::to_vec(&forged).unwrap(),
                    &context(),
                    &Deadline::new(30_000)
                )
                .is_err(),
                "{name}"
            );
        }
        for (start, end) in [(-1.0, 1.0), (2.0, 1.0), (0.0, 3600.1)] {
            let bad = report(json!([{"start":start,"end":end,"text":"original"}]));
            assert!(
                check(
                    &serde_json::to_vec(&bad).unwrap(),
                    &context(),
                    &Deadline::new(30_000)
                )
                .is_err()
            );
        }
    }
}
