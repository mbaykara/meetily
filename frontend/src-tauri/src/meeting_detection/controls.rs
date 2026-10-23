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
    teams_compose_toolbar: bool,
    teams_find: bool,
    teams_details: bool,
    teams_more: bool,
    teams_send: bool,
    zoom_back_home: bool,
    zoom_notes_activity: bool,
    zoom_editor_tab: bool,
    meeting_toolbar: bool,
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
                    self.teams_find |= label == "find in chat";
                    self.teams_details |= label == "open chat details";
                    self.teams_more |= label == "more chat options";
                    self.teams_send |= label == "send";
                    self.idle |= matches!(
                        label,
                        "join now" | "jetzt teilnehmen" | "an besprechung teilnehmen"
                    );
                }
                AppKind::Zoom => {
                    self.zoom_create |= label == "create new";
                    self.zoom_back_home |= label == "back to home";
                    self.zoom_notes_activity |= label == "view my notes activity center";
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
            self.zoom_editor_tab |= label == "notes";
            self.zoom_home_tab |= label == "home, 1 of 8";
            self.zoom_notes_tab |= label == "my notes, selected, 7 of 8";
        }
        if role == "axtoolbar" {
            self.meeting_toolbar |= label == "meeting controls";
            if matches!(app, AppKind::Teams) {
                self.teams_toolbar |= label == "chat actions";
                self.teams_compose_toolbar |= label == "actions for new message";
            }
        }
    }
    pub fn partial_call(&self) -> bool {
        self.leave || self.microphone || self.meeting_toolbar
    }
    pub fn observation(&self) -> Observation {
        if self.leave && self.microphone {
            Observation::Active
        } else if self.incomplete || self.partial_call() {
            Observation::Unknown
        } else if self.idle
            || (self.teams_toolbar && self.teams_meet)
            || (self.zoom_home_tab && self.zoom_notes_tab && self.zoom_create)
            || (self.teams_toolbar
                && self.teams_compose_toolbar
                && self.teams_find
                && self.teams_details
                && self.teams_more
                && self.teams_send)
            || (self.zoom_back_home
                && self.zoom_notes_activity
                && self.zoom_editor_tab
                && self.zoom_create)
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
    fn post_call_teams() -> WindowEvidence {
        let mut window = WindowEvidence::default();
        window.control(AppKind::Teams, "axtoolbar", "chat actions");
        window.control(AppKind::Teams, "axtoolbar", "actions for new message");
        window.control(AppKind::Teams, "axbutton", "find in chat");
        window.control(AppKind::Teams, "axbutton", "open chat details");
        window.control(AppKind::Teams, "axpopupbutton", "more chat options");
        window.control(AppKind::Teams, "axbutton", "send");
        window
    }
    fn post_call_zoom() -> WindowEvidence {
        let mut window = WindowEvidence::default();
        window.control(AppKind::Zoom, "axbutton", "back to home");
        window.control(AppKind::Zoom, "axbutton", "view my notes activity center");
        window.control(AppKind::Zoom, "axtab", "notes");
        window.control(AppKind::Zoom, "axbutton", "create new");
        window
    }
    #[test]
    fn observed_post_call_teams_chat_is_inactive_without_meet_now() {
        assert_eq!(post_call_teams().observation(), Observation::Inactive);
    }
    #[test]
    fn observed_post_call_zoom_notes_editor_is_inactive() {
        assert_eq!(post_call_zoom().observation(), Observation::Inactive);
    }
    #[test]
    fn partial_call_controls_cannot_be_overridden_by_idle_evidence() {
        let mut window = WindowEvidence::default();
        window.control(AppKind::Zoom, "axbutton", "join meeting");
        window.control(AppKind::Zoom, "axbutton", "leave meeting");
        assert_eq!(window.observation(), Observation::Unknown);
        let mut chat = post_call_teams();
        chat.control(AppKind::Teams, "axtoolbar", "meeting controls");
        assert_eq!(chat.observation(), Observation::Unknown);
    }
    #[test]
    fn post_call_idle_keeps_active_and_incomplete_window_precedence() {
        let idle = post_call_zoom();
        let mut call = WindowEvidence::default();
        call.control(AppKind::Zoom, "axbutton", "leave meeting");
        call.control(AppKind::Zoom, "axbutton", "mute");
        assert_eq!(
            windows_observation(&[idle.clone(), call], true),
            Observation::Active
        );
        let partial = WindowEvidence {
            incomplete: true,
            ..Default::default()
        };
        assert_eq!(
            windows_observation(&[idle, partial], true),
            Observation::Unknown
        );
    }
    #[test]
    fn both_observed_post_call_windows_allow_ten_second_owned_stop() {
        use super::super::policy::{combined, Action, Policy};
        let mut policy = Policy::default();
        let active = combined(&[Observation::Inactive, Observation::Active]);
        assert_eq!(policy.tick(0, true, active, None), None);
        assert_eq!(policy.tick(2, true, active, None), Some(Action::Start));
        policy.started(Some(42));
        let ended = combined(&[
            post_call_teams().observation(),
            post_call_zoom().observation(),
        ]);
        assert_eq!(policy.tick(4, true, ended, Some(42)), None);
        assert_eq!(policy.tick(12, true, ended, Some(42)), None);
        assert_eq!(
            policy.tick(14, true, ended, Some(42)),
            Some(Action::Stop(42))
        );
    }
    #[test]
    fn post_call_patterns_require_all_markers_and_complete_scans() {
        let mut teams = post_call_teams();
        teams.teams_send = false;
        assert_eq!(teams.observation(), Observation::Unknown);
        let mut zoom = post_call_zoom();
        zoom.zoom_editor_tab = false;
        assert_eq!(zoom.observation(), Observation::Unknown);
        let mut unreadable = post_call_zoom();
        unreadable.incomplete = true;
        assert_eq!(unreadable.observation(), Observation::Unknown);
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
