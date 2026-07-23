#![cfg_attr(all(test, not(target_os = "linux")), allow(dead_code))]

use thiserror::Error;

use crate::shared::{
    CredentialBackend, CredentialCapability, CredentialDeleteOutcome, CredentialDeleter,
    CredentialEnvelope, CredentialFailureKind, CredentialReader, CredentialStoreFailure,
    CredentialWriter, LoadedCredential,
};

use super::keyring::{CredentialStoreError as KeyringCredentialStoreError, NativeCredentialStore};

#[cfg(target_os = "linux")]
use super::xdg::{XdgCredentialStore, XdgCredentialStoreError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialAccessPurpose {
    Ordinary,
    Login,
    Logout,
}

pub(crate) enum CredentialStore {
    Keyring {
        native: NativeCredentialStore,
        #[cfg(target_os = "linux")]
        fallback: XdgCredentialStore,
        #[cfg(target_os = "linux")]
        purpose: CredentialAccessPurpose,
    },
    #[cfg(target_os = "linux")]
    Xdg(XdgCredentialStore),
}

impl CredentialStore {
    pub(crate) fn production(
        purpose: CredentialAccessPurpose,
    ) -> Result<Self, CredentialStoreSelectionError> {
        #[cfg(target_os = "linux")]
        {
            let fallback = XdgCredentialStore::production()
                .map_err(|source| CredentialStoreSelectionError::Xdg { source })?;
            return Self::select_with_probe(purpose, fallback, &DbusSecretServiceProbe);
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = purpose;
            Ok(Self::Keyring {
                native: NativeCredentialStore::new(),
            })
        }
    }

    #[must_use]
    pub(crate) const fn backend(&self) -> CredentialBackend {
        match self {
            Self::Keyring { .. } => CredentialBackend::Keyring,
            #[cfg(target_os = "linux")]
            Self::Xdg(_) => CredentialBackend::Xdg,
        }
    }

    /// Removes a superseded fallback only after the feature layer has completed its exact
    /// keyring save/read-back verification.
    pub(crate) fn retire_fallback_after_verified_login(&self) -> Result<(), CredentialStoreError> {
        #[cfg(target_os = "linux")]
        if let Self::Keyring { fallback, .. } = self {
            fallback
                .delete_credential()
                .map_err(|source| CredentialStoreError::Xdg { source })?;
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn select_with_probe<P: SecretServiceProbe>(
        purpose: CredentialAccessPurpose,
        fallback: XdgCredentialStore,
        probe: &P,
    ) -> Result<Self, CredentialStoreSelectionError> {
        let availability = probe
            .availability()
            .map_err(|source| CredentialStoreSelectionError::Probe { source })?;
        if availability.requires_keyring() {
            Ok(Self::Keyring {
                native: NativeCredentialStore::new(),
                fallback,
                purpose,
            })
        } else {
            Ok(Self::Xdg(fallback))
        }
    }
}

impl CredentialCapability for CredentialStore {
    type Error = CredentialStoreError;
}

impl CredentialReader for CredentialStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        match self {
            Self::Keyring {
                native,
                #[cfg(target_os = "linux")]
                fallback,
                #[cfg(target_os = "linux")]
                purpose,
            } => {
                #[cfg(target_os = "linux")]
                if fallback_requires_fresh_login(
                    *purpose,
                    fallback
                        .credential_exists()
                        .map_err(|source| CredentialStoreError::Xdg { source })?,
                ) {
                    return Err(CredentialStoreError::FallbackRequiresFreshLogin);
                }
                native
                    .read_credential()
                    .map_err(|source| CredentialStoreError::Keyring { source })
            }
            #[cfg(target_os = "linux")]
            Self::Xdg(store) => store
                .read_credential()
                .map_err(|source| CredentialStoreError::Xdg { source }),
        }
    }

    fn credential_backend(&self) -> CredentialBackend {
        self.backend()
    }
}

impl CredentialWriter for CredentialStore {
    fn save_credential(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error> {
        match self {
            Self::Keyring { native, .. } => native
                .save_credential(credential)
                .map_err(|source| CredentialStoreError::Keyring { source }),
            #[cfg(target_os = "linux")]
            Self::Xdg(store) => store
                .save_credential(credential)
                .map_err(|source| CredentialStoreError::Xdg { source }),
        }
    }
}

impl CredentialDeleter for CredentialStore {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        match self {
            Self::Keyring {
                native,
                #[cfg(target_os = "linux")]
                fallback,
                ..
            } => {
                #[cfg(target_os = "linux")]
                {
                    return delete_keyring_then_fallback(
                        || {
                            native
                                .delete_credential()
                                .map_err(|source| CredentialStoreError::Keyring { source })
                        },
                        || {
                            fallback
                                .delete_credential()
                                .map_err(|source| CredentialStoreError::Xdg { source })
                        },
                    );
                }
                #[cfg(not(target_os = "linux"))]
                native
                    .delete_credential()
                    .map_err(|source| CredentialStoreError::Keyring { source })
            }
            #[cfg(target_os = "linux")]
            Self::Xdg(store) => store
                .delete_credential()
                .map_err(|source| CredentialStoreError::Xdg { source }),
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn fallback_requires_fresh_login(purpose: CredentialAccessPurpose, fallback_exists: bool) -> bool {
    purpose == CredentialAccessPurpose::Ordinary && fallback_exists
}

#[cfg(any(target_os = "linux", test))]
fn combine_deletion_outcomes<E>(
    native: CredentialDeleteOutcome,
    delete_fallback: impl FnOnce() -> Result<CredentialDeleteOutcome, E>,
) -> Result<CredentialDeleteOutcome, E> {
    let fallback = delete_fallback()?;
    if native == CredentialDeleteOutcome::Deleted || fallback == CredentialDeleteOutcome::Deleted {
        Ok(CredentialDeleteOutcome::Deleted)
    } else {
        Ok(CredentialDeleteOutcome::Missing)
    }
}

#[cfg(any(target_os = "linux", test))]
fn delete_keyring_then_fallback<E>(
    delete_keyring: impl FnOnce() -> Result<CredentialDeleteOutcome, E>,
    delete_fallback: impl FnOnce() -> Result<CredentialDeleteOutcome, E>,
) -> Result<CredentialDeleteOutcome, E> {
    let native = delete_keyring()?;
    combine_deletion_outcomes(native, delete_fallback)
}

#[derive(Debug, Error)]
pub(crate) enum CredentialStoreError {
    #[error(transparent)]
    Keyring {
        #[from]
        source: KeyringCredentialStoreError,
    },

    #[cfg(target_os = "linux")]
    #[error(transparent)]
    Xdg {
        #[from]
        source: XdgCredentialStoreError,
    },

    #[cfg(target_os = "linux")]
    #[error(
        "an XDG fallback credential exists but a system keyring is now available; run `venmo auth login` to store a fresh keyring credential"
    )]
    FallbackRequiresFreshLogin,
}

impl CredentialStoreFailure for CredentialStoreError {
    fn kind(&self) -> CredentialFailureKind {
        match self {
            Self::Keyring { source } => source.kind(),
            #[cfg(target_os = "linux")]
            Self::Xdg { source } => source.kind(),
            #[cfg(target_os = "linux")]
            Self::FallbackRequiresFreshLogin => CredentialFailureKind::Unavailable,
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum CredentialStoreSelectionError {
    #[cfg(target_os = "linux")]
    #[error("failed to resolve the Linux fallback credential location")]
    Xdg {
        #[source]
        source: XdgCredentialStoreError,
    },

    #[cfg(target_os = "linux")]
    #[error("could not safely determine whether a Secret Service keyring is available")]
    Probe {
        #[source]
        source: SecretServiceProbeError,
    },

    #[cfg(not(target_os = "linux"))]
    #[allow(dead_code)]
    #[error("credential-store selection failed")]
    Unsupported,
}

#[cfg(any(target_os = "linux", all(test, unix)))]
const SECRET_SERVICE_NAME: &str = "org.freedesktop.secrets";

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SecretServiceAvailability {
    Running,
    Activatable,
    Absent,
}

#[cfg(any(target_os = "linux", test))]
impl SecretServiceAvailability {
    const fn requires_keyring(self) -> bool {
        matches!(self, Self::Running | Self::Activatable)
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
trait SecretServiceProbe {
    fn availability(&self) -> Result<SecretServiceAvailability, SecretServiceProbeError>;
}

#[cfg(any(target_os = "linux", all(test, unix)))]
struct DbusSecretServiceProbe;

#[cfg(any(target_os = "linux", all(test, unix)))]
impl SecretServiceProbe for DbusSecretServiceProbe {
    fn availability(&self) -> Result<SecretServiceAvailability, SecretServiceProbeError> {
        let connection = match zbus::blocking::Connection::session() {
            Ok(connection) => connection,
            Err(source) if session_bus_is_absent(&source) => {
                return Ok(SecretServiceAvailability::Absent);
            }
            Err(source) => return Err(SecretServiceProbeError::Connect { source }),
        };
        let proxy = zbus::blocking::fdo::DBusProxy::new(&connection)
            .map_err(|source| SecretServiceProbeError::Query { source })?;
        let service_name =
            zbus::names::BusName::from_static_str(SECRET_SERVICE_NAME).map_err(|source| {
                SecretServiceProbeError::Query {
                    source: source.into(),
                }
            })?;
        if proxy
            .name_has_owner(service_name)
            .map_err(|source| SecretServiceProbeError::Query {
                source: source.into(),
            })?
        {
            return Ok(SecretServiceAvailability::Running);
        }
        let activatable =
            proxy
                .list_activatable_names()
                .map_err(|source| SecretServiceProbeError::Query {
                    source: source.into(),
                })?;
        if activatable
            .iter()
            .any(|name| name.as_ref() == SECRET_SERVICE_NAME)
        {
            Ok(SecretServiceAvailability::Activatable)
        } else {
            Ok(SecretServiceAvailability::Absent)
        }
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn session_bus_is_absent(error: &zbus::Error) -> bool {
    let zbus::Error::InputOutput(source) = error else {
        return false;
    };
    matches!(
        source.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::NotConnected
    )
}

#[cfg(any(target_os = "linux", all(test, unix)))]
#[derive(Debug, Error)]
pub(crate) enum SecretServiceProbeError {
    #[error("failed to connect to the user D-Bus")]
    Connect {
        #[source]
        source: zbus::Error,
    },

    #[error("failed to query the user D-Bus for Secret Service availability")]
    Query {
        #[source]
        source: zbus::Error,
    },
}

#[cfg(test)]
mod tests {
    use std::io;
    #[cfg(unix)]
    use std::sync::Arc;

    use super::*;

    #[test]
    fn running_and_activatable_secret_services_require_the_keyring() {
        assert!(SecretServiceAvailability::Running.requires_keyring());
        assert!(SecretServiceAvailability::Activatable.requires_keyring());
        assert!(!SecretServiceAvailability::Absent.requires_keyring());
    }

    #[test]
    fn existing_fallback_blocks_only_ordinary_keyring_reads() {
        for purpose in [
            CredentialAccessPurpose::Ordinary,
            CredentialAccessPurpose::Login,
            CredentialAccessPurpose::Logout,
        ] {
            assert_eq!(
                fallback_requires_fresh_login(purpose, true),
                purpose == CredentialAccessPurpose::Ordinary
            );
            assert!(!fallback_requires_fresh_login(purpose, false));
        }
    }

    #[test]
    fn keyring_logout_outcome_includes_the_fallback_cleanup() {
        for (native, fallback, expected) in [
            (
                CredentialDeleteOutcome::Missing,
                CredentialDeleteOutcome::Missing,
                CredentialDeleteOutcome::Missing,
            ),
            (
                CredentialDeleteOutcome::Deleted,
                CredentialDeleteOutcome::Missing,
                CredentialDeleteOutcome::Deleted,
            ),
            (
                CredentialDeleteOutcome::Missing,
                CredentialDeleteOutcome::Deleted,
                CredentialDeleteOutcome::Deleted,
            ),
        ] {
            let observed = combine_deletion_outcomes::<io::Error>(native, || Ok(fallback));
            assert_eq!(observed.ok(), Some(expected));
        }
    }

    #[test]
    fn keyring_logout_never_deletes_the_fallback_after_a_keyring_failure() {
        let transcript = std::cell::RefCell::new(Vec::new());
        let result = delete_keyring_then_fallback(
            || {
                transcript.borrow_mut().push("keyring");
                Err(io::Error::other("synthetic keyring failure"))
            },
            || {
                transcript.borrow_mut().push("xdg");
                Ok(CredentialDeleteOutcome::Deleted)
            },
        );

        assert!(result.is_err());
        assert_eq!(transcript.borrow().as_slice(), ["keyring"]);
    }

    #[test]
    #[cfg(unix)]
    fn only_absent_session_bus_endpoints_are_treated_as_no_keyring() {
        for kind in [
            io::ErrorKind::NotFound,
            io::ErrorKind::ConnectionRefused,
            io::ErrorKind::NotConnected,
        ] {
            let error = zbus::Error::InputOutput(Arc::new(io::Error::from(kind)));
            assert!(session_bus_is_absent(&error));
        }

        for error in [
            zbus::Error::InputOutput(Arc::new(io::Error::from(io::ErrorKind::PermissionDenied))),
            zbus::Error::InputOutput(Arc::new(io::Error::from(io::ErrorKind::TimedOut))),
            zbus::Error::InvalidReply,
            zbus::Error::Address("synthetic invalid address".to_owned()),
        ] {
            assert!(!session_bus_is_absent(&error));
        }
    }
}
