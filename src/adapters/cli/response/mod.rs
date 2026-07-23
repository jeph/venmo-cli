use serde::{Serialize, Serializer};
use serde_json::Value;

#[cfg(test)]
use crate::features::activity::comment_remove::{
    ActivityCommentRemovalPlan, PreparedActivityCommentRemoval,
};
#[cfg(test)]
use crate::features::activity::reactions::{
    ActivityReactionPlan, PreparedActivityReactionMutation,
};
#[cfg(test)]
use crate::features::activity::social::{ActivitySocialPlan, PreparedActivitySocialMutation};
#[cfg(test)]
use crate::features::payments::PayPlan;
#[cfg(test)]
use crate::features::payments::pay::PreparedPay;
#[cfg(test)]
use crate::features::people::friendship::{FriendshipPlan, PreparedFriendshipMutation};
#[cfg(test)]
use crate::features::requests::accept::PreparedAccept;
#[cfg(test)]
use crate::features::requests::cancel::PreparedCancel;
#[cfg(test)]
use crate::features::requests::create::PreparedRequest;
#[cfg(test)]
use crate::features::requests::decline::PreparedDecline;
#[cfg(test)]
use crate::features::requests::{
    AcceptRequestPlan, CancelRequestPlan, CreateRequestPlan, DeclineRequestPlan,
};
#[cfg(test)]
use crate::features::transfers::model::TransferOutPlan;
#[cfg(test)]
use crate::features::transfers::out::PreparedTransferOut;

mod activity;
mod auth;
mod people;
mod requests;
mod shared;
mod transfers;
mod wallet;
mod writes;

pub(crate) use activity::{
    activity_comment_list, activity_comment_removal_plan, activity_comment_removal_result,
    activity_info, activity_list, activity_reaction_list, activity_reaction_plan,
    activity_reaction_result, activity_social_plan, activity_social_result,
};
pub(crate) use auth::{auth_status, logout, password_login};
pub(crate) use people::{friends, friendship_plan, friendship_result, user_info, user_search};
pub(crate) use requests::{request_info, requests};
pub(crate) use transfers::{transfer_options, transfer_out_plan, transfer_out_result};
pub(crate) use wallet::{balance, payment_methods};
pub(crate) use writes::{
    accept_plan, accept_result, cancel_plan, cancel_result, decline_plan, decline_result, pay_plan,
    pay_result, request_create_plan, request_create_result,
};

/// A frontend response keeps the source model available to the human renderer while exposing only
/// the explicitly constructed, safe JSON value to the machine renderer.
pub(crate) struct Response<'a, T: ?Sized> {
    source: &'a T,
    data: Value,
}

impl<'a, T: ?Sized> Response<'a, T> {
    pub(crate) const fn new(source: &'a T, data: Value) -> Self {
        Self { source, data }
    }

    pub(crate) const fn source(&self) -> &'a T {
        self.source
    }

    pub(crate) const fn data(&self) -> &Value {
        &self.data
    }
}

impl<T: ?Sized> std::fmt::Debug for Response<'_, T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Response([REDACTED])")
    }
}

impl<T: ?Sized> Serialize for Response<'_, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.data.serialize(serializer)
    }
}

pub(crate) trait HumanSource<T: ?Sized> {
    fn human_source(&self) -> &T;
}

#[cfg(test)]
impl<T: ?Sized> HumanSource<T> for T {
    fn human_source(&self) -> &T {
        self
    }
}

impl<T: ?Sized> HumanSource<T> for Response<'_, T> {
    fn human_source(&self) -> &T {
        self.source()
    }
}

#[cfg(test)]
macro_rules! prepared_source {
    ($prepared:ty, $plan:ty) => {
        impl HumanSource<$plan> for $prepared {
            fn human_source(&self) -> &$plan {
                self.plan()
            }
        }
    };
}

#[cfg(test)]
prepared_source!(PreparedPay, PayPlan);
#[cfg(test)]
prepared_source!(PreparedRequest, CreateRequestPlan);
#[cfg(test)]
prepared_source!(PreparedAccept, AcceptRequestPlan);
#[cfg(test)]
prepared_source!(PreparedDecline, DeclineRequestPlan);
#[cfg(test)]
prepared_source!(PreparedCancel, CancelRequestPlan);
#[cfg(test)]
prepared_source!(PreparedTransferOut, TransferOutPlan);
#[cfg(test)]
prepared_source!(PreparedFriendshipMutation, FriendshipPlan);
#[cfg(test)]
prepared_source!(PreparedActivitySocialMutation, ActivitySocialPlan);
#[cfg(test)]
prepared_source!(PreparedActivityReactionMutation, ActivityReactionPlan);
#[cfg(test)]
prepared_source!(PreparedActivityCommentRemoval, ActivityCommentRemovalPlan);

pub(crate) fn mutation_data(
    outcome: &'static str,
    performed: bool,
    plan: Value,
    result: Option<Value>,
) -> Value {
    serde_json::json!({
        "outcome": outcome,
        "performed": performed,
        "plan": plan,
        "result": result,
    })
}

pub(crate) fn dry_run<'a, T: ?Sized>(plan: &'a T, plan_data: Value) -> Response<'a, T> {
    Response::new(plan, mutation_data("dry_run", false, plan_data, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::wallet::SignedUsdAmount;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn response_serialization_never_visits_the_source_model() -> TestResult {
        struct SensitiveSource(&'static str);

        let source = SensitiveSource("synthetic-secret-source-value");
        let response = Response::new(&source, serde_json::json!({ "safe": true }));
        let encoded = serde_json::to_string(&response)?;

        assert_eq!(encoded, r#"{"safe":true}"#);
        assert!(!encoded.contains(source.0));
        assert_eq!(format!("{response:?}"), "Response([REDACTED])");
        Ok(())
    }

    #[test]
    fn wire_money_is_exact_at_signed_and_unsigned_boundaries() {
        assert_eq!(
            shared::unsigned_usd(u64::MAX),
            serde_json::json!({
                "amount": "184467440737095516.15",
                "currency": "USD",
            })
        );
        assert_eq!(
            shared::signed_usd(SignedUsdAmount::from_cents(i64::MIN)),
            serde_json::json!({
                "amount": "-92233720368547758.08",
                "currency": "USD",
            })
        );
        assert_eq!(
            shared::signed_usd(SignedUsdAmount::from_cents(0)),
            serde_json::json!({ "amount": "0.00", "currency": "USD" })
        );
    }

    #[test]
    fn timestamps_are_rfc3339_utc_and_dry_runs_have_explicit_null_results() -> TestResult {
        assert_eq!(
            shared::timestamp(time::OffsetDateTime::UNIX_EPOCH)?,
            "1970-01-01T00:00:00Z"
        );
        let plan = ();
        let response = dry_run(&plan, serde_json::json!({ "amount": "safe" }));
        assert_eq!(
            serde_json::to_value(response)?,
            serde_json::json!({
                "outcome": "dry_run",
                "performed": false,
                "plan": { "amount": "safe" },
                "result": null,
            })
        );
        Ok(())
    }
}
