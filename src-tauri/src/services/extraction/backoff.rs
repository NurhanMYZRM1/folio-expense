//! Pauses online extraction after the AI provider fails, so the receipts that
//! follow are read locally at once instead of each waiting out its own timeout.
use crate::error::AppError;
use std::time::{Duration, Instant};

/// How long a temporary failure (timeout, 429, 5xx, network) pauses online use.
const TRANSIENT_PAUSE: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The key, model or endpoint was refused (401/403/404): retrying cannot help
    /// until the user changes Settings or the credential.
    Rejected,
    /// The provider was slow, overloaded, rate-limited or unreachable.
    Transient,
}

impl FailureKind {
    /// `None` for errors that say nothing about the provider (a bad answer for
    /// one receipt, or our own validation).
    pub fn from_error(error: &AppError) -> Option<Self> {
        match error.code.as_str() {
            "AiProviderRejected" => Some(Self::Rejected),
            "AiProviderError" | "NetworkUnavailable" => Some(Self::Transient),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct OnlineBackoff {
    /// `Some(None)`: paused until `clear`; `Some(Some(t))`: paused until `t`.
    paused: Option<Option<Instant>>,
}

impl OnlineBackoff {
    pub fn record_failure(&mut self, kind: FailureKind, now: Instant) {
        match kind {
            FailureKind::Rejected => self.paused = Some(None),
            // Never shorten a pause that lasts until Settings change.
            FailureKind::Transient if self.paused == Some(None) => {}
            FailureKind::Transient => self.paused = Some(Some(now + TRANSIENT_PAUSE)),
        }
    }
    pub fn record_success(&mut self) {
        self.paused = None;
    }
    /// Settings or the credential changed: try the provider again.
    pub fn clear(&mut self) {
        self.paused = None;
    }
    /// Whether to skip the provider now. When a timed pause ends the breaker
    /// opens again, so the next receipt acts as the probe.
    pub fn is_paused(&mut self, now: Instant) -> bool {
        match self.paused {
            None => false,
            Some(None) => true,
            Some(Some(until)) if now < until => true,
            Some(Some(_)) => {
                self.paused = None;
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use std::time::{Duration, Instant};

    #[test]
    fn a_fresh_breaker_never_pauses() {
        let mut b = OnlineBackoff::default();
        assert!(!b.is_paused(Instant::now()));
    }

    #[test]
    fn a_temporary_failure_pauses_for_three_minutes_then_retries() {
        let mut b = OnlineBackoff::default();
        let t0 = Instant::now();
        b.record_failure(FailureKind::Transient, t0);
        assert!(b.is_paused(t0 + Duration::from_secs(1)));
        assert!(b.is_paused(t0 + Duration::from_secs(179)));
        assert!(
            !b.is_paused(t0 + Duration::from_secs(181)),
            "one probe after the pause"
        );
        assert!(
            !b.is_paused(t0 + Duration::from_secs(182)),
            "and it stays open until it fails again"
        );
    }

    #[test]
    fn a_rejected_key_or_model_pauses_until_settings_change() {
        let mut b = OnlineBackoff::default();
        let t0 = Instant::now();
        b.record_failure(FailureKind::Rejected, t0);
        assert!(b.is_paused(t0 + Duration::from_secs(3600 * 24)));
        b.clear();
        assert!(!b.is_paused(t0 + Duration::from_secs(3600 * 24)));
    }

    #[test]
    fn a_success_clears_a_pause() {
        let mut b = OnlineBackoff::default();
        let t0 = Instant::now();
        b.record_failure(FailureKind::Transient, t0);
        b.record_success();
        assert!(!b.is_paused(t0));
    }

    #[test]
    fn a_rejection_is_not_downgraded_by_a_later_transient_failure() {
        let mut b = OnlineBackoff::default();
        let t0 = Instant::now();
        b.record_failure(FailureKind::Rejected, t0);
        b.record_failure(FailureKind::Transient, t0);
        assert!(b.is_paused(t0 + Duration::from_secs(3600)));
    }

    #[test]
    fn only_provider_problems_count_as_failures() {
        let kind = |code: &str| FailureKind::from_error(&AppError::new(code, "x"));
        assert_eq!(kind("AiProviderRejected"), Some(FailureKind::Rejected));
        assert_eq!(kind("AiProviderError"), Some(FailureKind::Transient));
        assert_eq!(kind("NetworkUnavailable"), Some(FailureKind::Transient));
        // A bad answer for one receipt says nothing about the provider.
        assert_eq!(kind("InvalidExtraction"), None);
        assert_eq!(kind("ValidationError"), None);
    }
}
