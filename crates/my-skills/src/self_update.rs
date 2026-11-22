//! Release lookup and self-update library. Stream I. Spec §6.9.

/// One line for `doctor` (v0.4.1 prints `Update:  <state>`), for example
/// `up to date (stable v1.0.0)` or `not checked (--no-update-check)`.
/// Never fails: network or parse problems become a short "unknown (...)" text.
pub fn update_status_line(no_check: bool) -> String {
    if no_check {
        return "not checked (--no-update-check)".into();
    }
    "not checked (update lookup not implemented yet)".into()
}
