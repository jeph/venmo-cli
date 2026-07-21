mod codec;
mod keyring;
mod store;
#[cfg(any(target_os = "linux", test))]
mod xdg;

#[cfg(test)]
pub(crate) use keyring::NativeCredentialStore;
pub(crate) use store::{CredentialAccessPurpose, CredentialStore};
