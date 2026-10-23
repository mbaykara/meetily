pub(super) fn microphone_control(label: &str) -> bool {
    matches!(
        label,
        "mute"
            | "unmute"
            | "mute my audio"
            | "unmute my audio"
            | "turn mic off"
            | "turn mic on"
            | "stumm schalten"
            | "stummschaltung aufheben"
            | "mikrofon ausschalten"
            | "mikrofon einschalten"
    )
}
// Exact English/German call controls; intentionally conservative across versions.
pub(super) fn active_control(label: &str) -> bool {
    matches!(
        label,
        "leave"
            | "leave call"
            | "hang up"
            | "end meeting"
            | "leave meeting"
            | "verlassen"
            | "anruf verlassen"
            | "auflegen"
            | "meeting verlassen"
            | "besprechung verlassen"
    )
}
pub(super) fn idle_control(label: &str) -> bool {
    matches!(
        label,
        "join now"
            | "join meeting"
            | "new meeting"
            | "jetzt teilnehmen"
            | "an besprechung teilnehmen"
            | "neues meeting"
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_controls_only() {
        assert!(active_control("leave meeting"));
        assert!(microphone_control("mute"));
        assert!(microphone_control("unmute"));
        assert!(!microphone_control("microphone settings"));
        for s in [
            "daily meeting",
            "leave a message",
            "mute",
            "unmute",
            "teams",
            "joining",
        ] {
            assert!(!active_control(s));
        }
        assert!(idle_control("join now"));
    }
}
