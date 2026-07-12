use std::fmt;

use super::{UserId, Username};

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Eq, PartialEq)]
pub struct Account {
    user_id: UserId,
    username: Username,
    display_name: Option<String>,
}

impl Account {
    #[must_use]
    pub fn new(user_id: UserId, username: Username, display_name: Option<String>) -> Self {
        Self {
            user_id,
            username,
            display_name,
        }
    }

    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    #[must_use]
    pub fn username(&self) -> &Username {
        &self.username
    }

    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
}

impl fmt::Debug for Account {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Account")
            .field("user_id", &REDACTED)
            .field("username", &REDACTED)
            .field("display_name", &REDACTED)
            .finish()
    }
}
