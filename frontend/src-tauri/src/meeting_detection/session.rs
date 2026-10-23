use chrono::{DateTime, Local, TimeZone};
/// Freeze a session label once; never derive names from application windows.
pub fn session_name<T: TimeZone>(teams: bool, zoom: bool, started: &DateTime<T>) -> String
where
    T::Offset: std::fmt::Display,
{
    let application = match (teams, zoom) {
        (true, true) => "Teams and Zoom",
        (true, false) => "Teams",
        (false, true) => "Zoom",
        _ => "Meeting",
    };
    format!("{} {}", application, started.format("%Y-%m-%d_%H-%M-%S"))
}
pub fn local_session_name(teams: bool, zoom: bool) -> String {
    session_name(teams, zoom, &Local::now())
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    #[test]
    fn names_use_local_calendar_time_and_active_apps() {
        let local = FixedOffset::east_opt(7200)
            .unwrap()
            .with_ymd_and_hms(2026, 10, 23, 21, 50, 12)
            .unwrap();
        assert_eq!(
            session_name(true, false, &local),
            "Teams 2026-10-23_21-50-12"
        );
        assert_eq!(
            session_name(false, true, &local),
            "Zoom 2026-10-23_21-50-12"
        );
        assert_eq!(
            session_name(true, true, &local),
            "Teams and Zoom 2026-10-23_21-50-12"
        );
    }
}
