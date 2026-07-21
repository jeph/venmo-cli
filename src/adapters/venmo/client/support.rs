use std::collections::HashMap;
use std::str::FromStr;

use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, PrimitiveDateTime};

use crate::features::people::{FriendshipStatus, User, UserProfileKind};
use crate::shared::{Limit, UserId, Username};

use super::super::dto::UserDto;
use super::error::VenmoApiError;

const MAX_REMOTE_TEXT_BYTES: usize = 64 * 1024;

pub(super) fn map_users(
    users: Vec<UserDto>,
    operation: &'static str,
) -> Result<Vec<User>, VenmoApiError> {
    users
        .into_iter()
        .map(|user| map_user(user, operation))
        .collect()
}

pub(super) fn map_user(user: UserDto, operation: &'static str) -> Result<User, VenmoApiError> {
    let profile_kind = user.identity_type.as_deref().map(|value| {
        if value.eq_ignore_ascii_case("personal") {
            UserProfileKind::Personal
        } else if value.eq_ignore_ascii_case("business") {
            UserProfileKind::Business
        } else if value.eq_ignore_ascii_case("charity") {
            UserProfileKind::Charity
        } else {
            UserProfileKind::Unknown
        }
    });
    let is_payable = user.is_payable;
    let friendship_status = user.friend_status.as_deref().and_then(|value| match value {
        "friend" => Some(FriendshipStatus::Friend),
        "not_friend" => Some(FriendshipStatus::NotFriend),
        "request_received_by_you" => Some(FriendshipStatus::RequestReceived),
        "request_sent_by_you" => Some(FriendshipStatus::RequestSent),
        _ => None,
    });
    let user_id =
        UserId::from_str(&user.id.into_string()).map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the user response contained an invalid user ID",
        })?;
    let username = match user.username.filter(|value| !value.is_empty()) {
        Some(value) => {
            let bare = value.strip_prefix('@').unwrap_or(&value);
            Some(
                Username::from_bare(bare.to_owned()).map_err(|_| VenmoApiError::Contract {
                    operation,
                    problem: "the user response contained an invalid username",
                })?,
            )
        }
        None => None,
    };
    Ok(User::new(
        user_id,
        username,
        user.display_name.filter(|value| !value.is_empty()),
    )
    .with_optional_financial_attributes(profile_kind, is_payable)
    .with_optional_friendship_status(friendship_status))
}

pub(super) fn parse_required_timestamp(
    value: Option<&str>,
    operation: &'static str,
    problem: &'static str,
) -> Result<OffsetDateTime, VenmoApiError> {
    let value = value.ok_or(VenmoApiError::Contract { operation, problem })?;
    parse_timestamp(value, operation, problem)
}

pub(super) fn parse_timestamp(
    value: &str,
    operation: &'static str,
    problem: &'static str,
) -> Result<OffsetDateTime, VenmoApiError> {
    parse_timestamp_value(value).map_err(|()| VenmoApiError::Contract { operation, problem })
}

pub(super) fn parse_timestamp_value(value: &str) -> Result<OffsetDateTime, ()> {
    if let Ok(timestamp) = OffsetDateTime::parse(value, &Rfc3339) {
        return Ok(timestamp);
    }
    for description in [
        "[year]-[month]-[day]T[hour]:[minute]:[second]",
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]",
    ] {
        let format = time::format_description::parse_borrowed::<3>(description).map_err(|_| ())?;
        if let Ok(timestamp) = PrimitiveDateTime::parse(value, &format) {
            return Ok(timestamp.assume_utc());
        }
    }
    tracing::debug!(
        api.timestamp_length = value.len(),
        api.timestamp_has_fraction = value.contains('.'),
        api.timestamp_ends_in_z = value.ends_with('Z'),
        api.timestamp_has_space = value.contains(' '),
        "API timestamp did not match a supported format"
    );
    Err(())
}

pub(super) fn bounded_optional_text(
    value: Option<String>,
    operation: &'static str,
    problem: &'static str,
) -> Result<Option<String>, VenmoApiError> {
    if value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_REMOTE_TEXT_BYTES)
    {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

pub(super) fn bounded_required_text(
    value: String,
    operation: &'static str,
    problem: &'static str,
) -> Result<String, VenmoApiError> {
    if value.is_empty() || value.len() > MAX_REMOTE_TEXT_BYTES {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

pub(super) fn bounded_required_label(
    value: String,
    operation: &'static str,
    problem: &'static str,
) -> Result<String, VenmoApiError> {
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

pub(super) fn bounded_optional_label(
    value: Option<String>,
    operation: &'static str,
    problem: &'static str,
) -> Result<Option<String>, VenmoApiError> {
    if value.as_ref().is_some_and(|value| {
        value.is_empty() || value.len() > 64 || value.chars().any(char::is_control)
    }) {
        return Err(VenmoApiError::Contract { operation, problem });
    }
    Ok(value)
}

pub(super) fn validate_page_count(
    operation: &'static str,
    actual: usize,
    requested: Limit,
) -> Result<usize, VenmoApiError> {
    let requested = usize::try_from(requested.get()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the requested page size could not be represented safely",
    })?;
    if actual > requested {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the API returned more records than requested",
        });
    }
    Ok(requested)
}

pub(super) struct ValidatedQuery<'a> {
    values: HashMap<&'a str, &'a str>,
}

impl<'a> ValidatedQuery<'a> {
    pub(super) fn get(&self, name: &str) -> Option<&'a str> {
        self.values.get(name).copied()
    }
}

pub(super) fn validate_query_keys<'a>(
    operation: &'static str,
    pairs: &'a [(String, String)],
    allowed: &[&str],
) -> Result<ValidatedQuery<'a>, VenmoApiError> {
    let mut values = HashMap::with_capacity(pairs.len());
    for (key, value) in pairs {
        if !allowed.contains(&key.as_str()) || values.insert(key.as_str(), value.as_str()).is_some()
        {
            return Err(VenmoApiError::Contract {
                operation,
                problem: "the continuation link contained unexpected or duplicate query fields",
            });
        }
    }
    Ok(ValidatedQuery { values })
}

pub(super) fn require_query_value(
    operation: &'static str,
    query: &ValidatedQuery<'_>,
    name: &str,
    expected: &str,
) -> Result<(), VenmoApiError> {
    if query.get(name) != Some(expected) {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the continuation link changed a required query field",
        });
    }
    Ok(())
}

pub(super) fn require_query_value_case_insensitive(
    operation: &'static str,
    query: &ValidatedQuery<'_>,
    name: &str,
    expected: &str,
) -> Result<(), VenmoApiError> {
    if !query
        .get(name)
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the continuation link changed a required query field",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_parser_accepts_rfc3339_and_whole_or_fractional_naive_utc() {
        let rfc3339 = parse_timestamp_value("2026-07-11T12:00:00Z");
        let whole = parse_timestamp_value("2026-07-11T12:00:00");
        let fractional = parse_timestamp_value("2026-07-11T12:00:00.123456");

        assert_eq!(
            rfc3339.map(|value| value.unix_timestamp()),
            Ok(1_783_771_200)
        );
        assert_eq!(whole.map(|value| value.unix_timestamp()), Ok(1_783_771_200));
        assert_eq!(
            fractional.map(|value| (value.unix_timestamp(), value.nanosecond())),
            Ok((1_783_771_200, 123_456_000))
        );
        assert_eq!(parse_timestamp_value("invalid"), Err(()));
    }
}
