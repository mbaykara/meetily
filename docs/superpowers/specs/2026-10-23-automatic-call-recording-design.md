# Automatic call recording

Approved in conversation on 2026-10-23. Target: Microsoft Teams and Zoom desktop apps on macOS.

## Behavior

An opt-in Automatic recording section in Recording Settings has independent Teams and Zoom switches. Meetily must be running. Start recording after a supported app exposes confirmed in-call controls; stop and run the existing save/transcription/summary lifecycle after the call ends. Poll about every two seconds, require two positive samples to start, and ten seconds of confirmed absence to stop. Mute and silence are not end-of-call signals. App process presence alone and window titles alone cannot start recording.

Manual stop suppresses restarting until the detected call has ended. Recordings started manually remain under manual control. Simultaneous supported calls are treated as one recording until all selected apps leave their calls. Disabling automation while it owns a recording hands control to the user instead of discarding or stopping it. Start/stop commands must be explicit, never toggles. Never claim ownership of a manual recording after a race or failed automatic start.

## Detection

Use public macOS Accessibility APIs to identify in-call controls in the exact Teams and Zoom app bundle identifiers. Inspect control roles, identifiers and labels only; do not log or persist window contents, chat text, meeting titles, participants, or URLs. Do not read text bodies. Restrict traversal and IPC duration. A denied permission, unreadable UI, truncated scan, or stale result is Unknown, not an ended call. Recognized idle/prejoin state or an exited process is inactive. Unknown state does not trigger start/stop and resets confirmation timers.

Expose permission/detection status in settings and an explicit user action to open macOS Accessibility settings. Do not silently request/grant permissions. Initial label support is English with German equivalents where unambiguous; unsupported layouts must be visible as uncertain. Validate actual Teams/Zoom call signals on the installed versions before describing detection as verified. Live-call validation may require the user to run a solo call; synthetic tests do not establish app compatibility.

### Live-validation correction: session participants

End confirmation applies to applications positively observed active in the current call episode. Any selected application that becomes active joins the episode, and recording continues until all participants are confirmed inactive for the grace interval. Unknown from a participant resets that timer. Unknown from an unrelated application that has never been observed in this episode does not block stopping or rearming after a manual stop. Raw detection remains Unknown; empty windows, errors and scan limits are never treated as proof of inactivity. An entirely unrecognized overlapping call cannot be guaranteed to extend recording. This corrects the observed failure where a Zoom call ended but an unrelated unreadable Teams window kept recording indefinitely.

## Integration

Native polling must continue with the window hidden. Separate the native observation adapter, deterministic lifecycle policy, and Tauri orchestration. Use existing provider/model checks, device preferences, notification preferences, recording state events, and recording-stop-complete post-processing. Establish frontend readiness before starting, and synchronize transcript/session state for automatic starts. Serialize lifecycle actions against manual starts/stops with a token/session identity so automation cannot stop a replacement manual session. Failed starts are surfaced once per call and do not loop. Failed stops remain retryable and surface an error.

## Validation

Unit tests cover idle/prejoin, active call, mute/silence, reconnect grace, unknown/error samples, missing permission, manual stop suppression, manual session replacement, duplicate starts, failed start/stop, disabled settings, overlapping calls, and bounds on scan work. Test integration with the global frontend providers, not only the recording page. Run frontend TypeScript checks and relevant tests, Rust formatting/checks and policy tests. Record build and live-validation limits explicitly.

## User refinement: transcript-only sessions and dated names

The user requested date and time in call/file names and prioritized transcripts with no retained sound. Automatic Teams and Zoom sessions therefore capture audio for live transcription but do not persist audio or audio checkpoints, regardless of the manual recording audio preference. Preserve manual recording settings. Name each automatic session from its active app(s) and local start time, for example `Teams 2026-10-23_21-50-12`, `Zoom 2026-10-23_21-50-12`, or `Teams and Zoom 2026-10-23_21-50-12`. Use this name consistently in the meeting list, notification, and saved session folder. Never read meeting titles from app windows.

Persist structured transcripts and metadata incrementally through the existing save path. At stop, finalize transcripts and completed metadata even when no audio saver exists; retain error reporting and recovery behavior. Also produce a human-readable text transcript named after the dated session. Do not delete any existing audio files. Validate no audio/checkpoint files are created for new automatic sessions and the last transcript segments, duration and completed status are saved.

## User refinement: configurable storage folder

Recording Settings must show the saved calls folder for transcript-only and audio-saving sessions, with a working native folder picker and Open Folder action. Persist the chosen absolute directory across app restarts and validate that it can be used before confirming the change. A cancelled picker leaves preferences unchanged. A failed preference write leaves the prior setting and UI intact and shows an actionable error. New sessions snapshot this configured base directory at startup; the current session's output location never changes mid-session. Both manual and automatic starts honor the setting. Existing recordings stay in their original folders. The existing platform default applies until the user chooses another location.

## Constraints

- macOS desktop apps only; Windows/Linux compile with an unavailable detector.
- Off by default; Teams and Zoom individually configurable.
- Preserve upstream MIT notices. No emojis in new source/docs; Conventional Commits, no AI co-author.
- Do not modify the archived Python backend.
- No unrelated refactors, production data migration, default updater changes, or public posting.
