use crate::moon::config::MoonConfig;
use crate::moon::session_usage::SessionUsageSnapshot;
use crate::moon::state::MoonState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Archive,
    Compaction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextCompactionDecision {
    pub should_compact: bool,
    pub bypassed_cooldown: bool,
}

pub fn evaluate_context_compaction_candidate(
    usage_ratio: f64,
    start_ratio: f64,
    emergency_ratio: f64,
    cooldown_ready: bool,
) -> ContextCompactionDecision {
    if usage_ratio < start_ratio {
        return ContextCompactionDecision {
            should_compact: false,
            bypassed_cooldown: false,
        };
    }

    let bypassed_cooldown = !cooldown_ready && usage_ratio >= emergency_ratio;
    if cooldown_ready || bypassed_cooldown {
        return ContextCompactionDecision {
            should_compact: true,
            bypassed_cooldown,
        };
    }

    ContextCompactionDecision {
        should_compact: false,
        bypassed_cooldown: false,
    }
}

fn unified_layer1_last_trigger(state: &MoonState) -> Option<u64> {
    match (
        state.last_archive_trigger_epoch_secs,
        state.last_compaction_trigger_epoch_secs,
    ) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    }
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
    if usage.usage_ratio >= cfg.thresholds.trigger_ratio
        && should_fire(
            unified_layer1_last_trigger(state),
            now,
            cfg.watcher.cooldown_secs,
        )
    {
        // Unified trigger: archive-before-compact protocol.
        out.push(TriggerKind::Archive);
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
    fn evaluate_respects_unified_cooldown() {
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

        let mut state_in_cooldown = state.clone();
        state_in_cooldown.last_archive_trigger_epoch_secs = Some(995);
        state_in_cooldown.last_compaction_trigger_epoch_secs = Some(998);
        let triggers_cooldown = evaluate(&cfg, &state_in_cooldown, &usage);
        assert!(triggers_cooldown.is_empty());
    }

    #[test]
    fn context_compaction_bypasses_cooldown_only_on_emergency() {
        let start = 0.78;
        let emergency = 0.90;

        let regular = evaluate_context_compaction_candidate(0.85, start, emergency, false);
        assert!(!regular.should_compact);
        assert!(!regular.bypassed_cooldown);

        let emergency_hit = evaluate_context_compaction_candidate(0.95, start, emergency, false);
        assert!(emergency_hit.should_compact);
        assert!(emergency_hit.bypassed_cooldown);
    }

    #[test]
    fn context_compaction_retriggers_after_cooldown_when_still_above_start() {
        let start = 0.78;
        let emergency = 0.90;

        let ready = evaluate_context_compaction_candidate(0.82, start, emergency, true);
        assert!(ready.should_compact);
        assert!(!ready.bypassed_cooldown);
    }
}
