# Explicit local audio transcription

Code Intelligence 0.5.0 provides an optional host CLI/library ASR path. Default indexing and MCP calls start no ASR runtime. Package installation, model acquisition and execution are separate explicit host choices. The selected trusted process runs with ordinary host permissions; process ownership, deadlines and offline SDK settings are not an OS sandbox or independent runtime attestation.

Capture audio first with `yoke --root <source> media inspect <relative.wav>`. Use its immutable `assetId` for subsequent operations. This ASR contract accepts a nonempty, single-data-chunk, mono, 16 kHz PCM16 WAV of at most 600 seconds. The source file is not reopened for transcription; the registered bytes are checked and copied to a private temporary directory outside the analyzed source. Source changes are never applied.

Select an existing local Python runtime with faster-whisper 1.2.1, its program SHA-256, an existing local CTranslate2 model directory and exact model file names, byte sizes and SHA-256 values in a host configuration. The configuration is closed JSON and cannot grant execution. The program must be outside analyzed source; literal absolute host paths with nonlinked ancestors are required. A source-controlled executable or configuration flag cannot grant worker trust.

The host configuration fields are:

| Field | Contract |
| --- | --- |
| executable / executableSha256 | Explicit absolute program and SHA-256; checked before and after the attempt. |
| args | Trusted prefix arguments, normally ["-I"]. They do not select repository code implicitly. |
| modelRoot / modelFiles | Explicit local directory and 4-6 exact {name, bytes, sha256} entries. config.json, model.bin, tokenizer.json and vocabulary.txt or vocabulary.json are required. Optional preprocessor_config.json is supported. All present known inputs must be listed. |
| libraryVersion / language | 1.2.1 and an explicit two- or three-letter lowercase language code. |
| limits | Optional closed object: maxAudioMilliseconds (default 600000), maxSegments (512), maxTranscriptBytes (1000000), maxJobs (1000), threads (2, range 1-4). Limits cannot exceed these ceilings; zero segment/text budgets remain zero. |

The native process does not install packages or download weights. The Python helper selects the local directory with local_files_only, disables implicit tokens and uses offline SDK settings. It invokes CPU/int8 with one worker, beam size 1, no VAD or word timestamps. The reviewed PCM-array input bypasses the observed faster-whisper 1.2.1/PyAV 19 file-decoder incompatibility. Executable/model hashes bind the selected resources; the SDK version check does not attest every installed dependency or establish model correctness.

```
yoke --root <source> --timeout 120000 media transcribe <assetId> --config <absolute-host.json> --trust-worker
yoke --root <source> media asr-status <jobId>
yoke --root <source> media asr-read <jobId>
yoke --root <source> media asr-jobs <assetId> --offset 0 --limit 25
```

One transcribe invocation admits at most one attempt. It records a nonce-bound immutable request and a separate control journal before starting the owned process. Successful publication inserts the original response and independently checked transcript artifacts with a compare-and-swap control transition. Existing conflicting artifacts are refused. No caller transaction may span process execution. Stored read/status/jobs select no runtime or model, and replay completed output against captured audio and original response bytes. Pages have explicit bounds; limit 0 returns no entries without changing the total.

An admitted job has an unresolved outcome if the host stops before recording completion. A failed or cleanup_unproven job has no published transcript. No state triggers an automatic retry. cleanup_unproven retains its private directory for host recovery; neither that state nor an interrupted admitted state authorizes blind deletion or another model call. A finite metadata-only recovery budget cannot start another inference or extend the caller's original deadline.

Text is a selected local model observation, never a literal source quote or proof of observed execution. Original segment order, whitespace, duplicate observations and nominal timestamps are retained. Native sample envelopes separately intersect each nonzero nominal interval with captured audio using floor/ceil frame boundaries. A timestamp outside or partly outside the audio is explicitly classified; zero-length/outside intervals grant no positive sample span. These envelopes do not establish speech alignment. The actual owned speech trial retained an SDK end of 7.0 seconds over 6.19 seconds of audio, with the captured envelope ending at frame 99040 / byte 198126.

Retention validates paired typed journals, the captured WAV, original response and regenerated transcript. Exact technical inspection metadata can be reconstructed and checked. Only these validated CAS dependencies retain blobs; model/program hashes and inferred text do not grant retention authority. Corrupt or orphaned journals fail closed. Unrelated unknown artifacts retain the existing conservative behavior.

The dated validation record distinguishes controlled JSON transport fixtures from the actual selected faster-whisper/tiny.en CPU trial. The owned synthetic English utterance establishes a bounded working path, not general speech quality, multilingual accuracy, alternate formats, diarization, word alignment, OCR, scene understanding, hosted ASR or universal upstream parity.

Provider references: [faster-whisper](https://github.com/SYSTRAN/faster-whisper), [selected tiny.en model](https://huggingface.co/Systran/faster-whisper-tiny.en). Both declare MIT licensing; no provider application code or model weights are bundled.
