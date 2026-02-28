#![allow(dead_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OcOptimError {
    #[error("openclaw binary unavailable: {0}")]
    MissingOpenClawBinary(String),
    #[error("config file invalid or unreadable: {0}")]
    InvalidConfig(String),
    #[error("deterministic failure after retries: {0}")]
    DeterministicFailure(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoonErrorCode {
    E001Locked,
    E002StaleBuild,
    E003BinaryMismatch,
    E004CwdInvalid,
    E005ConfigMissing,
    E006DaemonPanic,
    E007StateCorrupt,
}

impl MoonErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::E001Locked => "E001_LOCKED",
            Self::E002StaleBuild => "E002_STALE_BUILD",
            Self::E003BinaryMismatch => "E003_BINARY_MISMATCH",
            Self::E004CwdInvalid => "E004_CWD_INVALID",
            Self::E005ConfigMissing => "E005_CONFIG_MISSING",
            Self::E006DaemonPanic => "E006_DAEMON_PANIC",
            Self::E007StateCorrupt => "E007_STATE_CORRUPT",
        }
    }
}
