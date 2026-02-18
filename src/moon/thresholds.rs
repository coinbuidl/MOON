use crate::moon::config::MoonConfig;
use crate::moon::session_usage::SessionUsageSnapshot;
use crate::moon::state::MoonState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Archive,
    Compaction,
}

fn should_fire(last_epoch: Option<u64>, now_epoch: u64, cooldown_secs: u64) -> bool {
    match last_epoch {
        None => true,
        Some(last) => now_epoch.saturating_sub(last) >= cooldown_secs,
    }
}

pub fn evaluate(
    cfg: &MoonConfig,
    state: &MoonState,
    usage: &SessionUsageSnapshot,
) -> Vec<TriggerKind> {
    let mut out = Vec::new();
    let now = usage.captured_at_epoch_secs;

    if cfg.thresholds.archive_ratio_trigger_enabled
        && usage.usage_ratio >= cfg.thresholds.archive_ratio
        && should_fire(
            state.last_archive_trigger_epoch_secs,
            now,
            cfg.watcher.cooldown_secs,
        )
    {
        out.push(TriggerKind::Archive);
    }

    if usage.usage_ratio >= cfg.thresholds.compaction_ratio
        && should_fire(
            state.last_compaction_trigger_epoch_secs,
            now,
            cfg.watcher.cooldown_secs,
        )
    {
        out.push(TriggerKind::Compaction);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moon::config::MoonConfig;

    #[test]
    fn evaluate_respects_order_and_thresholds() {
        let cfg = MoonConfig::default();
        let state = MoonState::default();
        let usage = SessionUsageSnapshot {
            session_id: "s".into(),
            used_tokens: 95,
            max_tokens: 100,
            usage_ratio: 0.95,
            captured_at_epoch_secs: 1000,
            provider: "t".into(),
        };

        let triggers = evaluate(&cfg, &state, &usage);
        assert_eq!(
            triggers,
            vec![TriggerKind::Archive, TriggerKind::Compaction]
        );
    }

    #[test]
    fn evaluate_skips_archive_when_archive_ratio_trigger_disabled() {
        let mut cfg = MoonConfig::default();
        cfg.thresholds.archive_ratio_trigger_enabled = false;

        let state = MoonState::default();
        let usage = SessionUsageSnapshot {
            session_id: "s".into(),
            used_tokens: 95,
            max_tokens: 100,
            usage_ratio: 0.95,
            captured_at_epoch_secs: 1000,
            provider: "t".into(),
        };

        let triggers = evaluate(&cfg, &state, &usage);
        assert_eq!(triggers, vec![TriggerKind::Compaction]);
    }
}
