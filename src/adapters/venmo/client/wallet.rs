use std::collections::HashSet;
use std::future::Future;
use std::str::FromStr;

use crate::features::payments::{
    PeerFundingApi, PeerFundingFee, PeerFundingMethod, PeerFundingRole, PeerFundingSources,
};
use crate::features::wallet::{
    Balance, BalanceApi, PaymentMethod, PaymentMethodId, PaymentMethodsApi, SignedUsdAmount,
};
use crate::shared::{AccessToken, DeviceId};

use super::super::dto::{BalanceEnvelope, FeeDto, PaymentMethodDto, PaymentMethodsEnvelope};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest};
use super::error::VenmoApiError;
use super::response::decode_success;
use super::{BALANCE_OPERATION, PAYMENT_METHODS_OPERATION, PEER_FUNDING_OPERATION, VenmoApiClient};

impl<T: ApiTransport> VenmoApiClient<T> {
    pub(super) async fn fetch_payment_methods(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Vec<PaymentMethod>, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/payment-methods", &["payment-methods"], &[]),
            )
            .await?;
        let envelope: PaymentMethodsEnvelope = decode_success(
            PAYMENT_METHODS_OPERATION,
            response,
            "the payment-method response did not match the supported envelope",
        )?;
        map_payment_methods(envelope.data.into_methods())
    }

    pub(super) async fn fetch_balance(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Balance, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/account", &["account"], &[]),
            )
            .await?;
        let envelope: BalanceEnvelope = decode_success(
            BALANCE_OPERATION,
            response,
            "the account response omitted the supported wallet-balance fields",
        )?;
        let available = SignedUsdAmount::from_str(&envelope.data.balance).map_err(|_| {
            VenmoApiError::Contract {
                operation: BALANCE_OPERATION,
                problem: "the account response contained an invalid available balance",
            }
        })?;
        let on_hold = SignedUsdAmount::from_str(&envelope.data.balance_on_hold).map_err(|_| {
            VenmoApiError::Contract {
                operation: BALANCE_OPERATION,
                problem: "the account response contained an invalid on-hold balance",
            }
        })?;
        Ok(Balance::new(available, on_hold))
    }

    async fn fetch_peer_funding_sources(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<PeerFundingSources, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/payment-methods", &["payment-methods"], &[]),
            )
            .await?;
        let envelope: PaymentMethodsEnvelope = decode_success(
            PEER_FUNDING_OPERATION,
            response,
            "the peer funding-method response did not match the supported envelope",
        )?;
        map_peer_funding_sources(envelope.data.into_methods())
    }
}

impl<T: ApiTransport> PaymentMethodsApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn payment_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a {
        self.fetch_payment_methods(access_token, device_id)
    }
}

impl<T: ApiTransport> PeerFundingApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn peer_funding_sources<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PeerFundingSources, Self::Error>> + Send + 'a {
        self.fetch_peer_funding_sources(access_token, device_id)
    }
}

impl<T: ApiTransport> BalanceApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn balance<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.fetch_balance(access_token, device_id)
    }
}

fn map_payment_methods(
    methods: Vec<PaymentMethodDto>,
) -> Result<Vec<PaymentMethod>, VenmoApiError> {
    let mut mapped = Vec::with_capacity(methods.len());
    let mut ids = HashSet::with_capacity(methods.len());
    for method in methods {
        let is_default = method.is_default();
        let id = PaymentMethodId::from_str(&method.id.into_string()).map_err(|_| {
            VenmoApiError::Contract {
                operation: PAYMENT_METHODS_OPERATION,
                problem: "the payment-method response contained an invalid method ID",
            }
        })?;
        if !ids.insert(id.clone()) {
            return Err(VenmoApiError::Contract {
                operation: PAYMENT_METHODS_OPERATION,
                problem: "the payment-method response contained duplicate method IDs",
            });
        }
        mapped.push(PaymentMethod::new(
            id,
            method.name.map(|value| value.into_string()),
            method.method_type.map(|value| value.into_string()),
            method.last_four.map(|value| value.into_string()),
            is_default,
        ));
    }
    Ok(mapped)
}

fn map_peer_funding_sources(
    methods: Vec<PaymentMethodDto>,
) -> Result<PeerFundingSources, VenmoApiError> {
    let mut mapped = Vec::with_capacity(methods.len());
    let mut balance = None;
    let mut ids = HashSet::with_capacity(methods.len());
    for method in methods {
        let PaymentMethodDto {
            id,
            name,
            method_type,
            last_four,
            is_default: _,
            role: _,
            payment_method_role: _,
            peer_payment_role,
            merchant_payment_role: _,
            fee,
        } = method;
        let id =
            PaymentMethodId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response contained an invalid method ID",
            })?;
        if !ids.insert(id.clone()) {
            return Err(VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response contained duplicate method IDs",
            });
        }
        let role = peer_payment_role.ok_or(VenmoApiError::Contract {
            operation: PEER_FUNDING_OPERATION,
            problem: "the peer funding-method response omitted a peer-payment role",
        })?;
        let role = match role.as_str().to_ascii_lowercase().as_str() {
            "default" => Some(PeerFundingRole::Default),
            "backup" => Some(PeerFundingRole::Backup),
            "none" => None,
            _ => {
                return Err(VenmoApiError::Contract {
                    operation: PEER_FUNDING_OPERATION,
                    problem: "the peer funding-method response contained an unknown peer-payment role",
                });
            }
        };
        let Some(role) = role else {
            continue;
        };
        let funding_kind = method_type
            .as_ref()
            .ok_or(VenmoApiError::Contract {
                operation: PEER_FUNDING_OPERATION,
                problem: "the peer funding-method response omitted the method type",
            })?
            .as_str()
            .to_ascii_lowercase();
        match funding_kind.as_str() {
            "balance" => {
                if balance.is_some() {
                    return Err(VenmoApiError::Contract {
                        operation: PEER_FUNDING_OPERATION,
                        problem: "the peer funding-method response contained multiple eligible Venmo balance sources",
                    });
                }
                balance = Some(PaymentMethod::new(
                    id,
                    name.map(|value| value.into_string()),
                    method_type.map(|value| value.into_string()),
                    last_four.map(|value| value.into_string()),
                    matches!(role, PeerFundingRole::Default),
                ));
                continue;
            }
            "bank" | "card" => {}
            _ => {
                return Err(VenmoApiError::Contract {
                    operation: PEER_FUNDING_OPERATION,
                    problem: "the peer funding-method response did not prove an external bank or card source",
                });
            }
        }
        let fee = match fee.as_ref().and_then(FeeDto::calculated_cents) {
            None => PeerFundingFee::Unknown,
            Some(0) => PeerFundingFee::ProvenZero,
            Some(cents) => PeerFundingFee::from_cents(cents),
        };
        let payment_method = PaymentMethod::new(
            id,
            name.map(|value| value.into_string()),
            method_type.map(|value| value.into_string()),
            last_four.map(|value| value.into_string()),
            matches!(role, PeerFundingRole::Default),
        );
        mapped.push(PeerFundingMethod::new(payment_method, role, fee));
    }
    Ok(PeerFundingSources::new(balance, mapped))
}
