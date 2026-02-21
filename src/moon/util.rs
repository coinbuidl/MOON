use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

/// Return the current Unix epoch in seconds.
///
/// This is the single, canonical implementation — **do not** duplicate
/// this helper in other modules.
pub fn now_epoch_secs() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

/// Truncate `input` to at most `max_chars` Unicode characters, stripping
/// control characters and appending `…` when truncated.
pub fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    let clean: String = input.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > max_chars {
        let mut s: String = clean.chars().take(max_chars).collect();
        s.push('…');
        s
    } else {
        clean
    }
}
