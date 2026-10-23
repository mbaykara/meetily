# Automatic recording for Teams and Zoom

Automatic recording transcribes calls without retaining sound. It is an optional macOS desktop feature. It is off by default. Meetily must remain running; hiding its window is supported. Quitting Meetily stops detection. Browser meetings and other operating systems are unsupported.

1. Complete Meetily onboarding, choose a transcription model, and verify a manual recording works with your preferred microphone and system audio devices.
2. Open Settings, then Recording Settings. Enable Microsoft Teams, Zoom, or both under Automatic recording.
3. Use Open Accessibility settings and enable Meetily in macOS Privacy & Security > Accessibility. Meetily does not grant or silently request this permission. Existing microphone and system audio permissions remain necessary.

Meetily polls native Accessibility controls about every two seconds. Two active observations start one recording. Confirmed absence for ten seconds stops it and triggers the existing save, transcription, and summary workflow. Mute and silence do not stop recording. Each selected app positively detected in a call joins the current call episode. Recording continues until all participating apps have been confirmed inactive for ten seconds. Unknown or unreadable detection for a participant resets the end timer and leaves recording running. A selected app that has never been detected active in this episode does not block another app’s confirmed end. Unsupported controls are displayed as unknown.

Only positively detected calls participate. An entirely unrecognized call in another app cannot be guaranteed to extend recording. Its raw status remains Unknown; this rule does not turn missing windows, permission failures or scan errors into inactivity.

A manual recording remains under manual control. Manually stopping an automatic recording prevents restarting during the same detected call. Disabling automation during recording hands control to you; use the normal Stop control to finish. A failed start is reported once per detected call; resolve the issue and finish the call before trying again, or start manually. A failed stop remains owned and retries after thirty seconds; the normal Stop control remains available.

The native detector limits traversal depth, element count, and IPC duration. Each window has its own scan budget; controls in different windows cannot form an active-call pair. Unreadable windows prevent confirmed call absence. It identifies only exact Teams/Zoom bundle identifiers and supported English/German control labels, requiring both a leave/end-call control and a microphone mute/unmute control. It does not use window titles, process presence, sound levels, browser tabs, or transcript content as active-call signals. It does not log or store inspected UI labels. Accessibility layout changes or localization can prevent detection.

A new recording waits for the previous recording to finish saving and complete its navigation cleanup. A failed save keeps the transcript available and blocks another start. Use Retry save in the error notification or Recording Settings to retry after resolving the error. A claimed preparation request reserves the native lifecycle until the frontend commits or cancels; reloading or closing the webview cancels the reservation.

## Calls folder

In Settings > Recording Settings, use Choose Folder under Calls folder to select the destination for new transcripts and optional manual audio. These controls remain available when manual audio saving is off. Open Folder opens the saved destination. The platform default is used until you choose another location.

The selected folder must be an absolute, writable local directory. Cancelling the picker or a failed preference write leaves the previous setting unchanged. The location persists across app restarts. Changing it affects new sessions only; each session keeps the destination captured when it starts. Stop the current recording before changing the folder. Existing recordings stay where they are and are never moved or deleted by this setting.

## Transcript files and names

Automatic calls use the app name(s) detected at recording start and local start date/time: `Teams 2026-10-23_21-50-12`, `Zoom 2026-10-23_21-50-12`, or `Teams and Zoom 2026-10-23_21-50-12`. The same stable name appears in the meeting list, start notification and saved session folder. A later overlapping call does not rename the existing session. Names never come from meeting windows.

Each automatic session folder contains `transcripts.json`, `metadata.json`, and a readable text file such as `Teams 2026-10-23_21-50-12.txt`. Text lines include elapsed timestamps. Finalization includes queued late transcript segments, completed metadata and recording duration. Audio is used for live transcription and discarded; automatic sessions do not create audio files or `.checkpoints`, even if Save Audio Recordings is enabled for manual recordings. Manual audio preferences remain unchanged, and existing audio files are kept.

Storage errors leave transcript buffers available and report an error. Retry stopping if native finalization failed; use Retry save if the later meeting/database save failed. Pending workers and transcript buffers remain tied to their original session while stopping is retried.

## Live validation checklist

Controls were observed through Accessibility on Teams 26276.505.5234.9924 during a muted solo call (`Leave` and `Unmute mic` in its meeting window), and on idle Teams and Zoom 7.2.2 (88465) on macOS 27.0.1. These observations informed fixtures. On installed build `2ecc52e`, the user verified Zoom automatic start and completed transcript-only JSON/TXT output after manually stopping. Leaving Zoom did not stop the recording automatically. A later installed `6d2cb2a` test confirmed Zoom automatic start and recognized Zoom idle after leaving, but Teams remained Unknown/NoWindows and blocked the old selected-app end policy. Participant-scoped end confirmation now has production-policy regression coverage; automatic stop still requires a real-call retest on the replacement build. Before relying on automation:

- Start a solo call in each app and confirm Settings reports active and Meetily starts exactly once.
- Mute, unmute, and remain silent; recording should continue.
- Hide the Meetily window and repeat a start and end cycle.
- Leave the call; allow ten seconds of confirmed inactive observations plus polling time for stopping and verify the meeting is saved and post-processing runs.
- Test overlapping Teams and Zoom calls; leaving only one must not stop recording.
- Stop manually during a call and verify it does not restart.
- Revoke Accessibility permission during a recording and verify unknown status leaves it running.
- Verify disabling automation hands over the current recording and a manual recording is never automatically stopped.

If a finished call remains unknown, stop Meetily manually and report the application version and language. Do not provide meeting contents or participant details. Do not assume compatibility from a process being installed or an idle window being readable.

## Detection diagnostics

Meetily logs a `Call detection transition` only when an app state, scan reason/completeness, selection, participant set or recording ownership changes. Reasons distinguish recognized idle screens, partial call evidence, unrecognized layouts, Accessibility errors and scan bounds. The participant mask is ordered Teams, then Zoom. These diagnostics contain fixed categories and native session counters, without inspected labels, window titles, chat/note text, participants or URLs.

A confirmed active call can show `complete: false` because scanning stops once its control pair is found. Confirmed inactivity always requires a complete scan of every relevant window. If automatic stop fails again, these categories establish whether the cause is an unknown layout or an incomplete/error scan; an observed idle fixture alone does not establish native scan completeness.

## Developer validation

The lifecycle policy is dependency-free and can be tested without compiling the audio stack:

```sh
rustc --test frontend/src-tauri/src/meeting_detection/policy.rs -o /tmp/meetily-policy-tests
/tmp/meetily-policy-tests
```

Frontend integration tests are in `frontend/tests/components/automatic-recording.test.tsx` and `frontend/tests/components/automatic-recording-lifecycle.test.tsx` and use the existing Bun/React test conventions. The complete native integration requires `cargo check --manifest-path frontend/src-tauri/Cargo.toml --lib`, including the project's audio/model toolchain dependencies.

Focused persistence/naming tests compile production source without the audio/Xcode stack:

```sh
cargo test --offline --manifest-path frontend/src-tauri/tests/transcript-persistence/Cargo.toml
```
