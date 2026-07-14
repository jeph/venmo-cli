use crate::features::doctor::BuildInfo;

pub(crate) const PRODUCTION_BUILD_INFO: BuildInfo = BuildInfo::new(
    env!("CARGO_PKG_VERSION"),
    std::env::consts::OS,
    std::env::consts::ARCH,
);
