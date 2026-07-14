#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
}

impl BuildInfo {
    #[must_use]
    pub const fn new(version: &'static str, os: &'static str, arch: &'static str) -> Self {
        Self { version, os, arch }
    }

    #[must_use]
    pub const fn version(self) -> &'static str {
        self.version
    }

    #[must_use]
    pub const fn os(self) -> &'static str {
        self.os
    }

    #[must_use]
    pub const fn arch(self) -> &'static str {
        self.arch
    }
}

pub enum ApiAvailability<'a, A> {
    Ready(&'a A),
    InitializationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoctorCheckStatus {
    Pass,
    Fail,
    Skipped,
}

impl DoctorCheckStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "Pass",
            Self::Fail => "Fail",
            Self::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DoctorCheck {
    name: &'static str,
    status: DoctorCheckStatus,
    detail: String,
    remediation: Option<&'static str>,
}

impl DoctorCheck {
    #[must_use]
    pub fn new(
        name: &'static str,
        status: DoctorCheckStatus,
        detail: impl Into<String>,
        remediation: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            status,
            detail: detail.into(),
            remediation,
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn status(&self) -> DoctorCheckStatus {
        self.status
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub const fn remediation(&self) -> Option<&'static str> {
        self.remediation
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DoctorReport {
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    #[must_use]
    pub fn new(checks: Vec<DoctorCheck>) -> Self {
        Self { checks }
    }

    #[must_use]
    pub fn checks(&self) -> &[DoctorCheck] {
        &self.checks
    }

    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.status() == DoctorCheckStatus::Pass)
    }
}
