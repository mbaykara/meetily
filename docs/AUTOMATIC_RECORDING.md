# Automatic recording for Teams and Zoom

Automatic recording is an optional macOS desktop feature. It is off by default. Meetily must remain running; hiding its window is supported. Quitting Meetily stops detection. Browser meetings and other operating systems are unsupported.

1. Complete Meetily onboarding, choose a transcription model, and verify a manual recording works with your preferred microphone and system audio devices.
2. Open Settings, then Recording Settings. Enable Microsoft Teams, Zoom, or both under Automatic recording.
3. Use Open Accessibility settings and enable Meetily in macOS Privacy & Security > Accessibility. Meetily does not grant or silently request this permission. Existing microphone and system audio permissions remain necessary.
4. Inform participants and follow your organization's recording rules before joining calls.

Meetily polls native Accessibility controls about every two seconds. Two active observations start one recording. Confirmed absence for ten seconds stops it and triggers the existing save, transcription, and summary workflow. Mute and silence do not stop recording. If both selected apps have calls, recording continues until both have ended. Unknown or unreadable detection resets confirmation timers and leaves a current recording running. Unsupported controls are displayed as unknown.

A manual recording remains under manual control. Manually stopping an automatic recording prevents restarting during the same detected call. Disabling automation during recording hands control to you; use the normal Stop control to finish. A failed start is reported once per detected call; resolve the issue and finish the call before trying again, or start manually. A failed stop remains owned and retries after thirty seconds; the normal Stop control remains available.

The native detector limits traversal depth, element count, and IPC duration. It identifies only exact Teams/Zoom bundle identifiers and supported English/German control labels, requiring both a leave/end-call control and a microphone mute/unmute control. It does not use window titles, process presence, sound levels, browser tabs, or transcript content as active-call signals. It does not log or store inspected UI labels. Accessibility layout changes or localization can prevent detection.

## Live validation checklist

Actual active-call detection on the installed Teams and Zoom versions has not been verified by synthetic tests. Before relying on automation:

- Start a solo call in each app and confirm Settings reports active and Meetily starts exactly once.
- Mute, unmute, and remain silent; recording should continue.
- Hide the Meetily window and repeat a start and end cycle.
- Leave the call; allow ten seconds of confirmed inactive observations plus polling time for stopping and verify the meeting is saved and post-processing runs.
- Test overlapping Teams and Zoom calls; leaving only one must not stop recording.
- Stop manually during a call and verify it does not restart.
- Revoke Accessibility permission during a recording and verify unknown status leaves it running.
- Verify disabling automation hands over the current recording and a manual recording is never automatically stopped.

If a finished call remains unknown, stop Meetily manually and report the application version and language. Do not provide meeting contents or participant details. Do not assume compatibility from a process being installed or an idle window being readable.

## Developer validation

The lifecycle policy is dependency-free and can be tested without compiling the audio stack:

```sh
rustc --test frontend/src-tauri/src/meeting_detection/policy.rs -o /tmp/meetily-policy-tests
/tmp/meetily-policy-tests
```

Frontend integration tests are in `frontend/tests/components/automatic-recording.test.tsx` and use the existing Bun/React test conventions. The complete native integration requires `cargo check --manifest-path frontend/src-tauri/Cargo.toml --lib`, including the project's audio/model toolchain dependencies.
