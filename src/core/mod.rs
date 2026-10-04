//! Core domain orchestration logic for Unarc.

pub mod app;
pub mod update;

pub use app::{AppInfo, Application, DiagnosticCheck, DoctorReport, EngineInfo};
pub use update::{
    DEFAULT_UPDATE_SOURCE, DownloadTransport, MockDownloadTransport, SystemCurlTransport,
    UpdateApplyResult, UpdateCheckResult, UpdateManager,
};
