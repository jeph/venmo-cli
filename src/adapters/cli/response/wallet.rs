use crate::features::wallet::balance::BalanceResult;
use crate::features::wallet::payment_methods::PaymentMethodsResult;

use super::Response;
use super::shared;

pub(crate) fn balance(result: &BalanceResult) -> Response<'_, BalanceResult> {
    Response::new(
        result,
        serde_json::json!({ "balance": shared::balance(result.balance()) }),
    )
}

pub(crate) fn payment_methods(result: &PaymentMethodsResult) -> Response<'_, PaymentMethodsResult> {
    Response::new(
        result,
        serde_json::json!({
            "payment_methods": result
                .methods()
                .iter()
                .map(shared::payment_method)
                .collect::<Vec<_>>(),
        }),
    )
}
