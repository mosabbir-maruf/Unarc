//! Core domain orchestration logic for Unarc.

pub mod app;
pub mod update;

pub use app::{AppInfo, Application, DiagnosticCheck, DoctorReport, EngineInfo};
pub use update::{
    DownloadTransport, MockDownloadTransport, SystemCurlTransport, UpdateApplyResult,
    UpdateCheckResult, UpdateManager, DEFAULT_UPDATE_SOURCE,
};
