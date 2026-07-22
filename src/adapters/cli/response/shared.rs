use std::io;

use serde_json::{Value, json};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

use crate::features::activity::model::ActivityDetailParties;
use crate::features::activity::{
    ActivityComment, ActivityCounterparty, ActivityDetail, ActivityDirection, ActivityFeedKind,
    ActivityLikeState, ActivitySocial, ActivitySocialCollection,
};
use crate::features::payments::{
    FinancialStatus, PeerFundingFee, PeerFundingRole, PeerFundingSource, PeerFundingSourceSelection,
};
use crate::features::people::{FriendshipStatus, User, UserProfileKind};
use crate::features::requests::{
    RequestAction, RequestDirection, RequestDirectionFilter, RequestRecord,
};
use crate::features::transfers::{
    TransferFeeMetadata, TransferInstrument, TransferModeOptions, TransferOutAmount, TransferSpeed,
};
use crate::features::wallet::{Balance, PaymentMethod, SignedUsdAmount};
use crate::shared::{Account, Money};

pub(super) fn timestamp(value: OffsetDateTime) -> io::Result<String> {
    value
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .map_err(io::Error::other)
}

pub(super) fn unsigned_usd(cents: u64) -> Value {
    json!({
        "amount": format!("{}.{:02}", cents / 100, cents % 100),
        "currency": "USD",
    })
}

pub(super) fn money(value: Money) -> Value {
    unsigned_usd(value.cents())
}

pub(super) fn signed_usd(value: SignedUsdAmount) -> Value {
    let cents = value.cents();
    let magnitude = cents.unsigned_abs();
    let sign = if cents < 0 { "-" } else { "" };
    json!({
        "amount": format!("{sign}{}.{:02}", magnitude / 100, magnitude % 100),
        "currency": "USD",
    })
}

pub(super) fn balance(value: &Balance) -> Value {
    json!({
        "available": signed_usd(value.available()),
        "on_hold": signed_usd(value.on_hold()),
    })
}

pub(super) fn account(value: &Account) -> Value {
    json!({
        "user_id": value.user_id().as_str(),
        "username": value.username().as_str(),
        "display_name": value.display_name(),
    })
}

pub(super) fn user(value: &User) -> Value {
    json!({
        "user_id": value.user_id().as_str(),
        "username": value.username().map(|username| username.as_str()),
        "display_name": value.display_name(),
        "profile_kind": value.profile_kind().map(profile_kind),
        "is_payable": value.is_payable(),
        "friendship_status": value.friendship_status().map(friendship_status),
    })
}

pub(super) const fn profile_kind(value: UserProfileKind) -> &'static str {
    match value {
        UserProfileKind::Personal => "personal",
        UserProfileKind::Business => "business",
        UserProfileKind::Charity => "charity",
        UserProfileKind::Unknown => "unknown",
    }
}

pub(super) const fn friendship_status(value: FriendshipStatus) -> &'static str {
    match value {
        FriendshipStatus::Friend => "friend",
        FriendshipStatus::NotFriend => "not_friend",
        FriendshipStatus::RequestReceived => "request_received",
        FriendshipStatus::RequestSent => "request_sent",
    }
}

pub(super) fn payment_method(value: &PaymentMethod) -> Value {
    json!({
        "id": value.id().as_str(),
        "name": value.name(),
        "type": value.method_type(),
        "last_four": value.last_four(),
        "is_default": value.is_default(),
    })
}

pub(super) fn funding_source(value: &PeerFundingSource) -> Value {
    let (kind, fee, role) = match value {
        PeerFundingSource::Balance(_) => ("balance", None, None),
        PeerFundingSource::External(method) => (
            "external",
            Some(funding_fee(method.fee())),
            Some(funding_role(method.role())),
        ),
    };
    json!({
        "kind": kind,
        "method": payment_method(value.method()),
        "role": role,
        "fee": fee,
    })
}

fn funding_fee(value: PeerFundingFee) -> Value {
    match value {
        PeerFundingFee::ProvenZero => json!({
            "status": "known",
            "amount": unsigned_usd(0),
        }),
        PeerFundingFee::NonZero { cents } => json!({
            "status": "known",
            "amount": unsigned_usd(cents.get()),
        }),
        PeerFundingFee::Unknown => json!({
            "status": "unknown",
            "amount": null,
        }),
    }
}

const fn funding_role(value: PeerFundingRole) -> &'static str {
    match value {
        PeerFundingRole::Default => "default",
        PeerFundingRole::Backup => "backup",
    }
}

pub(super) const fn funding_selection(value: PeerFundingSourceSelection) -> &'static str {
    match value {
        PeerFundingSourceSelection::Automatic => "automatic",
        PeerFundingSourceSelection::Explicit => "explicit",
    }
}

pub(super) const fn financial_status(value: FinancialStatus) -> &'static str {
    match value {
        FinancialStatus::Settled => "settled",
        FinancialStatus::Pending => "pending",
        FinancialStatus::Held => "held",
    }
}

pub(super) const fn activity_direction(value: ActivityDirection) -> &'static str {
    match value {
        ActivityDirection::Incoming => "incoming",
        ActivityDirection::Outgoing => "outgoing",
    }
}

pub(super) const fn activity_feed_kind(value: ActivityFeedKind) -> &'static str {
    match value {
        ActivityFeedKind::CurrentUser => "current_user",
        ActivityFeedKind::OtherPersonalUser => "other_personal_user",
    }
}

pub(super) const fn like_state(value: ActivityLikeState) -> &'static str {
    match value {
        ActivityLikeState::Liked => "liked",
        ActivityLikeState::NotLiked => "not_liked",
        ActivityLikeState::Unknown => "unknown",
    }
}

pub(super) fn activity_counterparty(value: &ActivityCounterparty) -> Value {
    match value {
        ActivityCounterparty::User(value) => json!({
            "kind": "user",
            "user": user(value),
        }),
        ActivityCounterparty::External {
            name,
            kind,
            last_four,
        } => json!({
            "kind": "external",
            "name": name,
            "type": kind,
            "last_four": last_four,
        }),
    }
}

pub(super) fn activity_comment(value: &ActivityComment) -> io::Result<Value> {
    Ok(json!({
        "id": value.id().as_str(),
        "author": user(value.author()),
        "message": value.message(),
        "created_at": timestamp(value.created_at())?,
    }))
}

fn social_collection<T>(
    value: &ActivitySocialCollection<T>,
    map: impl Fn(&T) -> io::Result<Value>,
) -> io::Result<Value> {
    let items = value
        .items()
        .iter()
        .map(map)
        .collect::<io::Result<Vec<_>>>()?;
    Ok(json!({
        "count": value.count(),
        "items": items,
        "complete": value.is_complete(),
    }))
}

pub(super) fn activity_social(value: &ActivitySocial) -> io::Result<Value> {
    let likes = value
        .likes()
        .map(|likes| social_collection(likes, |value| Ok(user(value))))
        .transpose()?;
    let comments = value
        .comments()
        .map(|comments| social_collection(comments, activity_comment))
        .transpose()?;
    Ok(json!({
        "likes": likes,
        "comments": comments,
    }))
}

pub(super) fn activity_detail(value: &ActivityDetail) -> io::Result<Value> {
    let parties = match value.parties() {
        ActivityDetailParties::Payment { actor, target } => json!({
            "kind": "payment",
            "actor": user(actor),
            "target": user(target),
        }),
        ActivityDetailParties::Relative {
            direction,
            counterparty,
        } => json!({
            "kind": "relative",
            "direction": activity_direction(*direction),
            "counterparty": activity_counterparty(counterparty),
        }),
        ActivityDetailParties::Account {
            account,
            direction,
            counterparty,
        } => json!({
            "kind": "account",
            "account": user(account),
            "direction": activity_direction(*direction),
            "counterparty": activity_counterparty(counterparty),
        }),
    };
    Ok(json!({
        "id": value.id().as_str(),
        "occurred_at": timestamp(value.occurred_at())?,
        "action": value.action().as_str(),
        "parties": parties,
        "amount": value.amount().map(money),
        "status": value.status().map(|status| status.as_str()),
        "note": value.note(),
        "audience": value.audience(),
        "social": activity_social(value.social())?,
    }))
}

pub(super) const fn request_action(value: RequestAction) -> &'static str {
    match value {
        RequestAction::Charge => "charge",
        RequestAction::Pay => "pay",
    }
}

pub(super) const fn request_direction(value: RequestDirection) -> &'static str {
    match value {
        RequestDirection::Incoming => "incoming",
        RequestDirection::Outgoing => "outgoing",
    }
}

pub(super) const fn request_direction_filter(value: RequestDirectionFilter) -> &'static str {
    match value {
        RequestDirectionFilter::All => "all",
        RequestDirectionFilter::Incoming => "incoming",
        RequestDirectionFilter::Outgoing => "outgoing",
    }
}

pub(super) fn request(value: &RequestRecord) -> io::Result<Value> {
    Ok(json!({
        "id": value.id().as_str(),
        "action": request_action(value.action()),
        "direction": request_direction(value.direction()),
        "counterparty": user(value.counterparty()),
        "amount": money(value.amount()),
        "note": value.note(),
        "audience": value.audience(),
        "created_at": value.created_at().map(timestamp).transpose()?,
        "status": value.status().as_str(),
    }))
}

pub(super) const fn transfer_speed(value: TransferSpeed) -> &'static str {
    match value {
        TransferSpeed::Standard => "standard",
        TransferSpeed::Instant => "instant",
    }
}

pub(super) fn transfer_instrument(value: &TransferInstrument) -> Value {
    json!({
        "id": value.id().as_str(),
        "name": value.name(),
        "asset_name": value.asset_name(),
        "type": value.instrument_type(),
        "last_four": value.last_four(),
        "is_default": value.is_default(),
        "estimated_completion": value.transfer_to_estimate(),
    })
}

fn transfer_fee(value: &TransferFeeMetadata) -> Value {
    json!({
        "minimum_amount": value.minimum_amount(),
        "maximum_amount": value.maximum_amount(),
        "variable_percentage": value.variable_percentage(),
        "has_additional_fields": value.has_additional_non_null_fields(),
    })
}

pub(super) fn transfer_mode(value: &TransferModeOptions) -> Value {
    json!({
        "eligible_destinations": value
            .eligible_destinations()
            .iter()
            .map(transfer_instrument)
            .collect::<Vec<_>>(),
        "fee": transfer_fee(value.fee()),
        "estimated_completion": value.transfer_to_estimate(),
    })
}

pub(super) fn transfer_amount_selection(value: TransferOutAmount) -> Value {
    match value {
        TransferOutAmount::Exact(amount) => json!({
            "kind": "exact",
            "amount": money(amount),
        }),
        TransferOutAmount::AllAvailable => json!({
            "kind": "all_available",
            "amount": null,
        }),
    }
}
