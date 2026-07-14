use serde::Deserialize;

use super::common::StringOrInteger;

#[derive(Deserialize)]
pub(crate) struct BalanceEnvelope {
    pub data: BalanceDto,
}

#[derive(Deserialize)]
pub(crate) struct BalanceDto {
    pub balance: String,
    pub balance_on_hold: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PaymentMethodsEnvelope {
    pub data: PaymentMethodsData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PaymentMethodsData {
    Wrapped {
        payment_methods: Vec<PaymentMethodDto>,
    },
    Direct(Vec<PaymentMethodDto>),
}

impl PaymentMethodsData {
    pub(crate) fn into_methods(self) -> Vec<PaymentMethodDto> {
        match self {
            Self::Wrapped { payment_methods } => payment_methods,
            Self::Direct(methods) => methods,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct PaymentMethodDto {
    pub id: StringOrInteger,
    #[serde(default, alias = "display_name", alias = "label")]
    pub name: Option<StringOrInteger>,
    #[serde(default, rename = "type", alias = "payment_method_type")]
    pub method_type: Option<StringOrInteger>,
    #[serde(default, alias = "lastFour")]
    pub last_four: Option<StringOrInteger>,
    #[serde(default, alias = "isDefault")]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub role: Option<StringOrInteger>,
    #[serde(default)]
    pub payment_method_role: Option<StringOrInteger>,
    #[serde(default)]
    pub peer_payment_role: Option<StringOrInteger>,
    #[serde(default)]
    pub merchant_payment_role: Option<StringOrInteger>,
    #[serde(default)]
    pub fee: Option<FeeDto>,
}

impl PaymentMethodDto {
    pub(crate) fn is_default(&self) -> bool {
        self.is_default == Some(true)
            || self
                .role_values()
                .flatten()
                .any(|role| role.as_str().to_ascii_lowercase().contains("default"))
    }

    fn role_values(&self) -> impl Iterator<Item = Option<&StringOrInteger>> {
        [
            self.role.as_ref(),
            self.payment_method_role.as_ref(),
            self.peer_payment_role.as_ref(),
            self.merchant_payment_role.as_ref(),
        ]
        .into_iter()
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum FeeDto {
    Calculated { calculated_fee_amount_in_cents: u64 },
    Unknown(serde_json::Value),
}

impl FeeDto {
    pub(crate) const fn calculated_cents(&self) -> Option<u64> {
        match self {
            Self::Calculated {
                calculated_fee_amount_in_cents,
            } => Some(*calculated_fee_amount_in_cents),
            Self::Unknown(value) => {
                let _ = value;
                None
            }
        }
    }
}
