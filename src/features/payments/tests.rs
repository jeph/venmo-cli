use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::auth::{PromptAvailability, PromptError};
use crate::features::payments::{
    BlankSourceEligibility, EligibilityToken, FinancialStatus, FinancialValidationError, PaymentId,
    PeerFundingFee, PeerFundingMethod, PeerFundingRole, PeerFundingSource,
    PeerFundingSourceSelection, PeerFundingSources,
};
use crate::features::people::recipients::RecipientResolutionFailureKind;
use crate::features::people::{
    User, UserProfileKind, UserSearchPage, UserSearchPageRequest, UserSearchQuery,
};
use crate::features::wallet::{Balance, PaymentMethod, PaymentMethodId, SignedUsdAmount};
use crate::shared::{
    AccessToken, Account, ApiFailureKind, ClientRequestId, CredentialAccessError,
    CredentialCapability, CredentialFailureKind, CredentialFormat, CredentialStoreFailure,
    DeviceId, Limit, LoadedCredential, Offset, UserId, Username,
};

#[path = "tests/outcomes.rs"]
mod outcomes;
#[path = "tests/support.rs"]
mod support;

use outcomes::*;
use support::*;

#[path = "tests/confirmation.rs"]
mod confirmation;
#[path = "tests/execute.rs"]
mod execute;
#[path = "tests/failures.rs"]
mod failures;
