"""Selected local CPU inference helper. The native caller owns admission and evidence."""

import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import sys
import wave


class Refusal(Exception):
    pass


def require(condition, code):
    if not condition:
        raise Refusal(code)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "asr_duplicate_request_key")
        result[key] = value
    return result


def closed(value, keys):
    require(type(value) is dict and set(value) == set(keys), "asr_request_schema")


def integer(value, minimum, maximum):
    require(type(value) is int and minimum <= value <= maximum, "asr_request_capacity")


def main():
    raw = sys.stdin.buffer.read(16_385)
    require(0 < len(raw) <= 16_384, "asr_request_capacity")
    request = json.loads(raw.decode("utf-8"), object_pairs_hook=unique_object)
    closed(request, ["schema", "jobId", "audio", "model", "language", "limits"])
    require(type(request["schema"]) is int and request["schema"] == 1, "asr_request_schema")
    require(
        type(request["jobId"]) is str
        and len(request["jobId"]) == 64
        and all(char in "0123456789abcdef" for char in request["jobId"]),
        "asr_request_identity",
    )
    audio = request["audio"]
    model = request["model"]
    limits = request["limits"]
    closed(audio, ["path", "sha256", "frames"])
    closed(model, ["directory", "libraryVersion"])
    closed(limits, ["threads", "maxSegments", "maxTranscriptBytes"])
    integer(audio["frames"], 1, 9_600_000)
    integer(limits["threads"], 1, 4)
    integer(limits["maxSegments"], 0, 512)
    integer(limits["maxTranscriptBytes"], 0, 1_000_000)
    require(
        type(request["language"]) is str
        and 2 <= len(request["language"]) <= 3
        and all("a" <= char <= "z" for char in request["language"]),
        "asr_language_selection",
    )
    require(
        type(model["libraryVersion"]) is str
        and 1 <= len(model["libraryVersion"]) <= 100,
        "asr_library_version_selection",
    )
    require(type(audio["path"]) is str and type(model["directory"]) is str, "asr_request_path")
    audio_path = Path(audio["path"])
    directory = Path(model["directory"])
    require(audio_path.is_absolute() and directory.is_absolute(), "asr_request_path")
    require(
        directory.is_dir()
        and all((directory / name).is_file() for name in ["config.json", "model.bin", "tokenizer.json"])
        and any((directory / name).is_file() for name in ["vocabulary.txt", "vocabulary.json"]),
        "asr_local_model_bundle_incomplete",
    )
    require(audio_path.stat().st_size <= 50_000_000, "asr_audio_capacity")
    with audio_path.open("rb") as selected_bytes:
        captured = selected_bytes.read(50_000_001)
    require(len(captured) <= 50_000_000, "asr_audio_capacity")
    audio_hash = hashlib.sha256(captured).hexdigest()
    require(audio_hash == audio["sha256"], "asr_audio_hash_mismatch")
    with wave.open(str(audio_path), "rb") as selected:
        require(
            selected.getnchannels() == 1
            and selected.getsampwidth() == 2
            and selected.getframerate() == 16_000
            and selected.getcomptype() == "NONE"
            and selected.getnframes() == audio["frames"],
            "asr_pcm_shape_unavailable",
        )
        pcm = selected.readframes(audio["frames"] + 1)
    require(len(pcm) == audio["frames"] * 2, "asr_pcm_shape_unavailable")
    os.environ.update(
        HF_HUB_OFFLINE="1",
        HF_HUB_DISABLE_IMPLICIT_TOKEN="1",
        HF_HUB_DISABLE_TELEMETRY="1",
        TRANSFORMERS_OFFLINE="1",
        DO_NOT_TRACK="1",
        OMP_NUM_THREADS=str(limits["threads"]),
    )
    version = importlib.metadata.version("faster-whisper")
    require(version == model["libraryVersion"], "asr_library_version_mismatch")
    import numpy as np
    from faster_whisper import WhisperModel

    samples = np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0
    selected = WhisperModel(
        str(directory), device="cpu", compute_type="int8",
        cpu_threads=limits["threads"], num_workers=1,
        local_files_only=True, use_auth_token=False,
    )
    require(selected.feature_extractor.sampling_rate == 16_000, "asr_model_sample_rate_unavailable")
    observed, info = selected.transcribe(
        samples, language=request["language"], beam_size=1, best_of=1,
        temperature=0.0, condition_on_previous_text=False, vad_filter=False,
        word_timestamps=False, log_progress=False,
    )
    segments = []
    text_bytes = 0
    for segment in observed:
        require(len(segments) < limits["maxSegments"], "asr_segment_capacity")
        require(type(segment.text) is str and segment.text.strip(), "asr_segment_text_invalid")
        text_bytes += len(segment.text.encode("utf-8"))
        require(text_bytes <= limits["maxTranscriptBytes"], "asr_transcript_capacity")
        start, end = float(segment.start), float(segment.end)
        require(math.isfinite(start) and math.isfinite(end) and 0 <= start <= end <= 3600,
                "asr_reported_timestamp_invalid")
        segments.append({"start": start, "end": end, "text": segment.text})
    # Keep nominal SDK timestamps, including padding beyond the input duration.
    # Native evidence separately qualifies their overlap with captured audio.
    output = {
        "schema": 1, "jobId": request["jobId"], "libraryVersion": version,
        "audioSha256": audio_hash, "audioFrames": audio["frames"], "sampleRate": 16_000,
        "language": info.language, "durationSeconds": float(info.duration),
        "requestedComputeType": "int8", "segments": segments,
    }
    encoded = json.dumps(output, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
    require(len(encoded) <= 2_000_000, "asr_response_capacity")
    sys.stdout.buffer.write(encoded + b"\n")


if __name__ == "__main__":
    try:
        main()
    except Refusal as error:
        sys.stderr.write(str(error) + "\n")
        sys.exit(2)
    except BaseException:
        # Do not expose model paths, environment values or provider exceptions.
        sys.stderr.write("asr_local_worker_failed\n")
        sys.exit(2)
