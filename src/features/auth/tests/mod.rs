use std::cell::RefCell;
use std::error::Error as StdError;
use std::fmt;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;

use super::*;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, ApplicationFailureKind, Clock,
    CredentialCapability, CredentialDeleteOutcome, CredentialDeleter, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure,
    CredentialWriter, DeviceId, LoadedCredential, UserId, Username,
};

type TestResult = Result<(), Box<dyn StdError>>;

mod fixtures;
mod outcomes;
mod support;

use fixtures::*;
use outcomes::*;
use support::*;

mod password;
mod status_logout;
