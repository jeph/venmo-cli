use std::io::Write;

use crate::adapters::system::PRODUCTION_BUILD_INFO;
use crate::features::doctor::{self as doctor, DoctorApi};
use crate::shared::CredentialReader;

use super::super::{error::AppError, output};
use super::composition::ProductionProvider;

pub(super) async fn run_production<W: Write>(
    provider: ProductionProvider,
    stdout: &mut W,
) -> Result<(), AppError> {
    let store = provider.credential_store();
    let api_result = provider.api();
    let api = match &api_result {
        Ok(api) => doctor::ApiAvailability::Ready(api),
        Err(_) => doctor::ApiAvailability::InitializationFailed,
    };
    run_with(&store, api, PRODUCTION_BUILD_INFO, stdout).await
}

async fn run_with<S, A, W>(
    store: &S,
    api: doctor::ApiAvailability<'_, A>,
    build_info: doctor::BuildInfo,
    stdout: &mut W,
) -> Result<(), AppError>
where
    S: CredentialReader,
    A: DoctorApi,
    W: Write,
{
    let report = doctor::diagnose(store, api, build_info).await;
    output::write_doctor(stdout, &report)?;
    if report.is_healthy() {
        Ok(())
    } else {
        Err(AppError::DoctorIncomplete)
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
