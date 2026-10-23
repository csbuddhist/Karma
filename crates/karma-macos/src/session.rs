//! The user at the active graphical session.

use std::os::unix::fs::MetadataExt;

/// The first uid macOS assigns to a regular account. Lower uids belong to
/// root and system daemons, which Karma never treats as the monitored user.
pub const FIRST_REGULAR_UID: u32 = 501;

/// The uid that owns the console, which macOS hands to the active graphical
/// session (also across fast user switching), or `None` at the login window.
pub fn console_user() -> Option<u32> {
    let uid = std::fs::metadata("/dev/console").ok()?.uid();
    is_regular_user(uid).then_some(uid)
}

/// Whether `uid` is a regular account rather than root or a system daemon.
pub fn is_regular_user(uid: u32) -> bool {
    uid >= FIRST_REGULAR_UID && uid != u32::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_regular_accounts_count_as_users() {
        assert!(!is_regular_user(0));
        assert!(!is_regular_user(88));
        assert!(!is_regular_user(500));
        assert!(is_regular_user(501));
        assert!(!is_regular_user(u32::MAX));
    }
}
