# Automatic Call Recording Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Automatically record supported Teams and Zoom desktop calls on macOS and stop/save when they end.

**Architecture:** A bounded native Accessibility adapter reports active, inactive, or unknown observations. A deterministic policy owns only automatic sessions. A native polling loop coordinates explicit recording commands with the existing global frontend lifecycle.

**Tech Stack:** Rust, Tauri 2, macOS ApplicationServices/AppKit, React/TypeScript, Bun tests.

**Spec:** docs/superpowers/specs/2026-10-23-automatic-call-recording-design.md

## Global Constraints

- macOS desktop apps only; Windows/Linux compile with an unavailable detector.
- Off by default; Teams and Zoom individually configurable.
- Preserve upstream MIT notices. No emojis in new source/docs; Conventional Commits, no AI co-author.
- Do not modify the archived Python backend.
- No unrelated refactors, production data migration, default updater changes, or public posting.

### Task 1: Detector, lifecycle and settings as one integrated feature

**Files:**
- Create `frontend/src-tauri/src/meeting_detection/` for native adapter, pure policy, Tauri orchestration and tests.
- Modify `frontend/src-tauri/src/lib.rs` for setup/commands and lifecycle coordination.
- Modify `frontend/src-tauri/src/audio/recording_commands.rs` only where session ownership/serialization needs it.
- Create `frontend/src/components/AutomaticRecordingSettings.tsx` and a global automation bridge if required.
- Modify `frontend/src/components/RecordingSettings.tsx` and `frontend/src/app/layout.tsx` to mount UI/bridge.
- Create relevant tests under existing frontend test conventions and `docs/AUTOMATIC_RECORDING.md`.

**Interfaces:**
- Detector observations distinguish Active, Inactive and Unknown, independently for Teams and Zoom.
- The settings command returns supported platform, accessibility permission and app observation status along with preferences.
- Native loop must not depend on a timer running in a hidden webview.
- Existing Rust start APIs emit recording-starting/recording-started; existing stop APIs emit recording-stopped. Emit recording-stop-complete only after a successful automatic stop so the global RecordingPostProcessingProvider runs.
- Existing TranscriptContext consumes recording-started but does not clear old transcript buffers: ensure automatic start preparation explicitly coordinates this.

- [ ] Write deterministic lifecycle tests before implementation, including the following invariant checks:

```text
idle -> active -> active: one Start
owned recording -> Unknown for 30s: no Stop
owned recording -> Inactive for 8s -> Active: no Stop
owned recording -> Inactive for 10s: one Stop
manual stop -> still Active: no restart
failed auto start racing manual start: no ownership, no later auto stop
stop failure: show error and preserve a retry path
disable while owned: relinquish ownership without stop
Teams ends while Zoom remains active: continue
```

- [ ] Run failing tests; implement a pure policy with monotonic durations and action acknowledgements, then rerun.
- [ ] Implement native adapter with per-app identity, bounded traversal, exact call-control matching, permission status and Unknown handling. Do not treat arbitrary title matches, browser tabs, silence or app presence as active calls.
- [ ] Add persisted preferences, native polling and session-safe integration. Prefer a narrow lifecycle lock/session token over broad changes to recording internals. Check onboarding and frontend readiness before automatic starts; preserve provider/device selection.
- [ ] Add settings controls, status/error copy and accessibility settings action. Match existing components. Add tests for persistence failure and unavailable/permission-denied cases where meaningful.
- [ ] Run targeted tests, TypeScript checks and Rust checks. The installed Rust binaries are under `/Users/mbaykara/.cargo/bin`. Report existing baseline failures separately.
- [ ] Write user setup and live-validation instructions. Never claim real-call detection was verified based only on synthetic tests.
- [ ] Self-review diff and commit only task files with `feat(recording): detect Teams and Zoom calls` (or multiple concise Conventional Commits if warranted).

## Execution record

### Task 2: Transcript-only automatic sessions and dated transcript files

User-directed refinement during packaging: use automatic-call audio only for transcription and label session/file with local start date and time. Extend Task 1 without changing manual recording preferences or existing files.

1. Generate a safe stable session name from observed active Teams/Zoom apps and local start time; reuse for start, notification, persistence and readable transcript filename.
2. Make automatic start explicitly disable retained audio/checkpoints; preserve manual audio preference behavior.
3. Repair transcript-only saver finalization: persist last segments, validate output, finalize completed metadata/duration and report errors without requiring an audio path. Add a readable text transcript beside structured JSON.
4. Add focused tests for naming, no-audio artifacts and final transcript/metadata persistence; run relevant compile/frontend checks. Update user documentation and commit a concise Conventional Commit.
5. Review the focused diff and fix material findings before pushing for the new installer. Parent owns CI fixes and build monitoring; implementer owns Task 2 source/tests/docs.

Design approved by user. Both applications installed. Teams idle UI inspected through computer-use: no active call. Active-call controls remain to be validated. Rust toolchain exists outside default PATH. Parent agent handles toolchain setup and independent validation while the implementer owns feature changes.
