use std::fmt;

use super::PaymentMethodId;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Eq, PartialEq)]
pub struct PaymentMethod {
    id: PaymentMethodId,
    name: Option<String>,
    method_type: Option<String>,
    last_four: Option<String>,
    is_default: bool,
}

impl PaymentMethod {
    #[must_use]
    pub fn new(
        id: PaymentMethodId,
        name: Option<String>,
        method_type: Option<String>,
        last_four: Option<String>,
        is_default: bool,
    ) -> Self {
        Self {
            id,
            name,
            method_type,
            last_four,
            is_default,
        }
    }

    #[must_use]
    pub fn id(&self) -> &PaymentMethodId {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub fn method_type(&self) -> Option<&str> {
        self.method_type.as_deref()
    }

    #[must_use]
    pub fn last_four(&self) -> Option<&str> {
        self.last_four.as_deref()
    }

    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.is_default
    }
}

impl fmt::Debug for PaymentMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaymentMethod")
            .field("id", &REDACTED)
            .field("name", &REDACTED)
            .field("method_type", &REDACTED)
            .field("last_four", &REDACTED)
            .field("is_default", &self.is_default)
            .finish()
    }
}
