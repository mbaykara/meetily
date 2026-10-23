use super::policy::Observation;
#[derive(Clone, Copy, Debug)]
pub enum AppKind {
    Teams,
    Zoom,
}
#[derive(Default, Clone)]
pub struct WindowEvidence {
    leave: bool,
    microphone: bool,
    idle: bool,
    teams_toolbar: bool,
    teams_meet: bool,
    zoom_home_tab: bool,
    zoom_notes_tab: bool,
    zoom_create: bool,
    pub incomplete: bool,
}
impl WindowEvidence {
    pub fn control(&mut self, app: AppKind, role: &str, label: &str) {
        if role == "axbutton" || role == "axmenubutton" || role == "axpopupbutton" {
            self.leave |= active_control(label);
            self.microphone |= microphone_control(label);
            match app {
                AppKind::Teams => {
                    self.teams_meet |= label == "meet now";
                    self.idle |= matches!(
                        label,
                        "join now" | "jetzt teilnehmen" | "an besprechung teilnehmen"
                    );
                }
                AppKind::Zoom => {
                    self.zoom_create |= label == "create new";
                    self.idle |= matches!(
                        label,
                        "join meeting"
                            | "new meeting"
                            | "start a new meeting with video on"
                            | "neues meeting"
                    );
                }
            }
        }
        if matches!(app, AppKind::Zoom) && matches!(role, "axtab" | "axradiobutton") {
            self.zoom_home_tab |= label == "home, 1 of 8";
            self.zoom_notes_tab |= label == "my notes, selected, 7 of 8";
        }
        if matches!(app, AppKind::Teams) && role == "axtoolbar" && label == "chat actions" {
            self.teams_toolbar = true;
        }
    }
    pub fn observation(&self) -> Observation {
        if self.leave && self.microphone {
            Observation::Active
        } else if !self.incomplete
            && (self.idle
                || (self.teams_toolbar && self.teams_meet)
                || (self.zoom_home_tab && self.zoom_notes_tab && self.zoom_create))
        {
            Observation::Inactive
        } else {
            Observation::Unknown
        }
    }
}
pub fn windows_observation(windows: &[WindowEvidence], complete: bool) -> Observation {
    if windows
        .iter()
        .any(|window| window.observation() == Observation::Active)
    {
        Observation::Active
    } else if complete
        && !windows.is_empty()
        && windows
            .iter()
            .all(|window| window.observation() == Observation::Inactive)
    {
        Observation::Inactive
    } else {
        Observation::Unknown
    }
}
fn microphone_control(label: &str) -> bool {
    matches!(
        label,
        "mute"
            | "unmute"
            | "mute mic"
            | "unmute mic"
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
fn active_control(label: &str) -> bool {
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
#[cfg(test)]
mod tests {
    use super::*;
    fn teams_shell() -> WindowEvidence {
        let mut window = WindowEvidence::default();
        window.control(AppKind::Teams, "axtoolbar", "chat actions");
        window.control(AppKind::Teams, "axbutton", "meet now");
        window
    }
    #[test]
    fn verified_teams_shell_and_live_muted_call() {
        let shell = teams_shell();
        assert_eq!(shell.observation(), Observation::Inactive);
        let mut call = WindowEvidence::default();
        call.control(AppKind::Teams, "axbutton", "leave");
        call.control(AppKind::Teams, "axbutton", "unmute mic");
        assert_eq!(
            windows_observation(&[shell, call], true),
            Observation::Active
        );
    }
    #[test]
    fn partial_call_window_overrides_idle_shell() {
        let call = WindowEvidence {
            incomplete: true,
            ..Default::default()
        };
        assert_eq!(
            windows_observation(&[teams_shell(), call], true),
            Observation::Unknown
        );
        assert_eq!(
            windows_observation(&[teams_shell()], false),
            Observation::Unknown
        );
    }
    #[test]
    fn controls_cannot_pair_across_windows() {
        let mut a = WindowEvidence::default();
        let mut b = WindowEvidence::default();
        a.control(AppKind::Teams, "axbutton", "leave");
        b.control(AppKind::Teams, "axbutton", "unmute mic");
        assert_eq!(windows_observation(&[a, b], true), Observation::Unknown);
    }
    #[test]
    fn verified_zoom_home() {
        let mut home = WindowEvidence::default();
        home.control(AppKind::Zoom, "axbutton", "join meeting");
        assert_eq!(home.observation(), Observation::Inactive);
        home.incomplete = true;
        assert_eq!(home.observation(), Observation::Unknown);
    }
    #[test]
    fn verified_zoom_notes_shell() {
        let mut notes = WindowEvidence::default();
        notes.control(AppKind::Zoom, "axtab", "home, 1 of 8");
        notes.control(AppKind::Zoom, "axtab", "my notes, selected, 7 of 8");
        notes.control(AppKind::Zoom, "axbutton", "create new");
        assert_eq!(notes.observation(), Observation::Inactive);
        assert_eq!(
            windows_observation(
                &[
                    notes,
                    WindowEvidence {
                        incomplete: true,
                        ..Default::default()
                    }
                ],
                true
            ),
            Observation::Unknown
        );
    }
    #[test]
    fn arbitrary_labels_cannot_start() {
        for label in [
            "daily meeting",
            "leave a message",
            "mute",
            "teams",
            "joining",
        ] {
            assert!(!active_control(label));
        }
    }
}
