use std::collections::BTreeSet;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::*;
use crate::adapters::credentials::NativeCredentialStore;
use crate::shared::CredentialReader;

#[tokio::test(flavor = "current_thread")]
#[ignore = "manually probes production read-only response shapes with the active keychain credential"]
async fn live_read_only_schema_probe() -> TestResult {
    let loaded = NativeCredentialStore::new()
        .read_credential()?
        .ok_or_else(|| io::Error::other("the live schema probe requires a stored credential"))?;
    let client = VenmoApiClient::production()?;
    let session = ApiSession::from(&loaded.envelope);
    let user_id = loaded.envelope.user_id().as_str();

    let _ = probe_shape(
        &client,
        session,
        "account",
        HttpRequest::read("/account", &["account"], &[]),
    )
    .await?;
    let friends = probe_shape(
        &client,
        session,
        "friends",
        HttpRequest::read(
            "/users/{user-id}/friends",
            &["users", user_id, "friends"],
            &[("limit", "2"), ("offset", "0")],
        ),
    )
    .await?;
    summarize_next_link("friends", friends.as_ref());
    let activity = probe_shape(
        &client,
        session,
        "activity",
        HttpRequest::read(
            "/stories/target-or-actor/{user-id}",
            &["stories", "target-or-actor", user_id],
            &[("limit", "2"), ("social_only", "false")],
        ),
    )
    .await?;
    summarize_next_link("activity", activity.as_ref());
    summarize_timestamp_shapes("activity", activity.as_ref());
    if let Some(value) = activity.as_ref() {
        if let Some(story_id) = value.pointer("/data/0/id").and_then(Value::as_str) {
            let _ = probe_shape(
                &client,
                session,
                "story-detail",
                HttpRequest::read("/stories/{story-id}", &["stories", story_id], &[]),
            )
            .await?;
        }
        if let Some(payment_id) = value.pointer("/data/0/payment/id").and_then(Value::as_str) {
            let _ = probe_shape(
                &client,
                session,
                "activity-payment-detail",
                HttpRequest::read("/payments/{payment-id}", &["payments", payment_id], &[]),
            )
            .await?;
        }
    }
    let pending = probe_shape(
        &client,
        session,
        "pending-requests",
        HttpRequest::read(
            "/payments",
            &["payments"],
            &[
                ("action", "charge"),
                ("status", "pending,held"),
                ("limit", "1"),
            ],
        ),
    )
    .await?;
    summarize_next_link("pending-requests", pending.as_ref());
    summarize_request_directions(pending.as_ref(), user_id);
    if let Some(request_id) = pending
        .as_ref()
        .and_then(|value| value.pointer("/data/0/id"))
        .and_then(Value::as_str)
    {
        let _ = probe_shape(
            &client,
            session,
            "pending-request-detail",
            HttpRequest::read("/payments/{payment-id}", &["payments", request_id], &[]),
        )
        .await?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "manually probes production activity continuation semantics with the active credential"]
async fn live_activity_continuation_probe() -> TestResult {
    let loaded = NativeCredentialStore::new()
        .read_credential()?
        .ok_or_else(|| {
            io::Error::other("the live continuation probe requires a stored credential")
        })?;
    let client = VenmoApiClient::production()?;
    let limit = "11";
    let response = client
        .transport
        .send_authenticated(
            ApiSession::from(&loaded.envelope),
            HttpRequest::read(
                "/stories/target-or-actor/{user-id}",
                &[
                    "stories",
                    "target-or-actor",
                    loaded.envelope.user_id().as_str(),
                ],
                &[("limit", limit), ("social_only", "false")],
            ),
        )
        .await?;
    let value: Value = serde_json::from_slice(response.body())?;
    summarize_next_link("activity-continuation", Some(&value));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "manually probes non-payment activity shapes with the active credential"]
async fn live_non_payment_activity_probe() -> TestResult {
    let loaded = NativeCredentialStore::new()
        .read_credential()?
        .ok_or_else(|| {
            io::Error::other("the live activity-shape probe requires a stored credential")
        })?;
    let client = VenmoApiClient::production()?;
    let response = client
        .transport
        .send_authenticated(
            ApiSession::from(&loaded.envelope),
            HttpRequest::read(
                "/stories/target-or-actor/{user-id}",
                &[
                    "stories",
                    "target-or-actor",
                    loaded.envelope.user_id().as_str(),
                ],
                &[("limit", "50"), ("social_only", "false")],
            ),
        )
        .await?;
    let value: Value = serde_json::from_slice(response.body())?;
    let records = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| io::Error::other("activity probe did not return an array"))?;
    let mut types = std::collections::BTreeMap::<String, u32>::new();
    let mut first_non_payment = None;
    for record in records {
        let story_type = record
            .get("type")
            .and_then(Value::as_str)
            .and_then(safe_enum_value)
            .unwrap_or("unknown");
        *types.entry(story_type.to_owned()).or_default() += 1;
        if story_type != "payment" && first_non_payment.is_none() {
            first_non_payment = Some(record);
        }
    }
    eprintln!("schema-probe activity type-counts: {types:?}");
    if let Some(record) = first_non_payment {
        let mut shape = BTreeSet::new();
        collect_json_shape(record, "$.data[]", None, 0, &mut shape);
        for line in shape {
            eprintln!("  {line}");
        }
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "manually locates unsupported activity structures without emitting values"]
async fn live_activity_contract_failure_probe() -> TestResult {
    let loaded = NativeCredentialStore::new()
        .read_credential()?
        .ok_or_else(|| {
            io::Error::other("the live activity-contract probe requires a stored credential")
        })?;
    let client = VenmoApiClient::production()?;
    let page_size = Limit::try_from(50)?;
    let mut token: Option<ActivityBeforeId> = None;
    for page_index in 0_u8..4 {
        let mut query = vec![("limit", "50"), ("social_only", "false")];
        if let Some(before_id) = token.as_ref() {
            query.push(("before_id", before_id.as_str()));
        }
        let path_segments = [
            "stories",
            "target-or-actor",
            loaded.envelope.user_id().as_str(),
        ];
        let response = client
            .transport
            .send_authenticated(
                ApiSession::from(&loaded.envelope),
                HttpRequest::read("/stories/target-or-actor/{user-id}", &path_segments, &query),
            )
            .await?;
        let value: Value = serde_json::from_slice(response.body())?;
        let records = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| io::Error::other("activity probe did not return an array"))?;
        for record in records {
            let supported = serde_json::from_value::<StoryDto>(record.clone())
                .ok()
                .is_some_and(|story| map_activity(story, loaded.envelope.user_id()).is_ok());
            if !supported {
                eprintln!(
                    "schema-probe unsupported activity record on bounded page {}:",
                    page_index + 1
                );
                let mut shape = BTreeSet::new();
                collect_json_shape(record, "$.data[]", None, 0, &mut shape);
                for line in shape {
                    eprintln!("  {line}");
                }
                return Ok(());
            }
        }
        token = client.parse_activity_next_link(
            value.pointer("/pagination/next").and_then(Value::as_str),
            &path_segments,
            page_size,
            ActivityFeedKind::CurrentUser,
        )?;
        if token.is_none() {
            eprintln!("schema-probe activity contract: all bounded records were supported");
            return Ok(());
        }
    }
    eprintln!("schema-probe activity contract: no unsupported record in four bounded pages");
    Ok(())
}

async fn probe_shape(
    client: &VenmoApiClient,
    session: ApiSession<'_>,
    label: &str,
    request: HttpRequest<'_>,
) -> Result<Option<Value>, Box<dyn Error>> {
    let response = client
        .transport
        .send_authenticated(session, request)
        .await?;
    eprintln!("schema-probe {label}: HTTP {}", response.status().as_u16());
    if response.body().is_empty() {
        eprintln!("  $: empty-body");
        return Ok(None);
    }
    let value: Value = match serde_json::from_slice(response.body()) {
        Ok(value) => value,
        Err(_) => {
            eprintln!("  $: non-json-body");
            return Ok(None);
        }
    };
    let mut shape = BTreeSet::new();
    collect_json_shape(&value, "$", None, 0, &mut shape);
    for line in shape {
        eprintln!("  {line}");
    }
    Ok(Some(value))
}

fn summarize_next_link(label: &str, value: Option<&Value>) {
    let Some(next) = value
        .and_then(|value| value.pointer("/pagination/next"))
        .and_then(Value::as_str)
    else {
        eprintln!("schema-probe {label} pagination: no-next-link");
        return;
    };
    let Ok(url) = reqwest::Url::parse(next) else {
        eprintln!("schema-probe {label} pagination: unparseable-next-link");
        return;
    };
    let trusted_origin = url.scheme() == "https"
        && url.host_str() == Some("api.venmo.com")
        && url.port_or_known_default() == Some(443);
    let query_keys = url
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .collect::<BTreeSet<_>>();
    let safe_values = url
        .query_pairs()
        .filter_map(|(key, value)| {
            matches!(
                key.as_ref(),
                "action" | "limit" | "offset" | "only_public_stories" | "social_only" | "status"
            )
            .then(|| format!("{key}={value}"))
        })
        .collect::<BTreeSet<_>>();
    eprintln!(
        "schema-probe {label} pagination: trusted-origin={trusted_origin} query-keys={query_keys:?} safe-values={safe_values:?}"
    );
}

fn summarize_request_directions(value: Option<&Value>, user_id: &str) {
    let Some(records) = value
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    else {
        eprintln!("schema-probe pending-requests directions: unavailable");
        return;
    };
    let mut incoming = 0_u32;
    let mut outgoing = 0_u32;
    let mut unknown = 0_u32;
    for record in records {
        let actor = record.pointer("/actor/id").and_then(Value::as_str);
        let target = record.pointer("/target/user/id").and_then(Value::as_str);
        if actor == Some(user_id) {
            outgoing = outgoing.saturating_add(1);
        } else if target == Some(user_id) {
            incoming = incoming.saturating_add(1);
        } else {
            unknown = unknown.saturating_add(1);
        }
    }
    eprintln!(
        "schema-probe pending-requests directions: incoming={incoming} outgoing={outgoing} unknown={unknown}"
    );
}

fn summarize_timestamp_shapes(label: &str, value: Option<&Value>) {
    let Some(records) = value
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    else {
        eprintln!("schema-probe {label} timestamps: unavailable");
        return;
    };
    let mut shapes = BTreeSet::new();
    for record in records {
        for (field, candidate) in [
            ("story.date_created", record.get("date_created")),
            (
                "payment.date_created",
                record.pointer("/payment/date_created"),
            ),
        ] {
            match candidate.and_then(Value::as_str) {
                Some(value) => {
                    shapes.insert(format!("{field}: {}", timestamp_shape(value)));
                }
                None => {
                    shapes.insert(format!("{field}: absent-or-non-string"));
                }
            }
        }
    }
    eprintln!("schema-probe {label} timestamps: {shapes:?}");
}

fn timestamp_shape(value: &str) -> String {
    let timezone = if value.ends_with('Z') {
        "zulu"
    } else if value
        .get(10..)
        .is_some_and(|suffix| suffix.contains('+') || suffix.rfind('-').is_some())
    {
        "numeric-offset"
    } else {
        "no-offset"
    };
    let fractional_digits = value.split_once('.').map_or(0, |(_, suffix)| {
        suffix.bytes().take_while(u8::is_ascii_digit).count()
    });
    format!(
        "bytes={} has-T={} timezone={timezone} fractional-digits={fractional_digits} rfc3339={}",
        value.len(),
        value.contains('T'),
        OffsetDateTime::parse(value, &Rfc3339).is_ok()
    )
}

fn collect_json_shape(
    value: &Value,
    path: &str,
    field: Option<&str>,
    depth: usize,
    shape: &mut BTreeSet<String>,
) {
    const MAX_DEPTH: usize = 12;
    if depth > MAX_DEPTH {
        shape.insert(format!("{path}: depth-limit"));
        return;
    }
    match value {
        Value::Null => {
            shape.insert(format!("{path}: null"));
        }
        Value::Bool(_) => {
            shape.insert(format!("{path}: bool"));
        }
        Value::Number(_) => {
            shape.insert(format!("{path}: number"));
        }
        Value::String(value) => {
            let enum_value = field
                .filter(|field| is_allowlisted_enum_field(field))
                .and_then(|_| safe_enum_value(value));
            match enum_value {
                Some(value) => {
                    shape.insert(format!("{path}: string enum={value}"));
                }
                None => {
                    shape.insert(format!("{path}: string"));
                }
            }
        }
        Value::Array(values) => {
            shape.insert(format!("{path}: array length={}", values.len()));
            for value in values {
                collect_json_shape(value, &format!("{path}[]"), field, depth + 1, shape);
            }
        }
        Value::Object(fields) => {
            shape.insert(format!("{path}: object"));
            for (key, value) in fields {
                let safe_key = safe_schema_key(key).unwrap_or("[dynamic-key]");
                collect_json_shape(
                    value,
                    &format!("{path}.{safe_key}"),
                    Some(safe_key),
                    depth + 1,
                    shape,
                );
            }
        }
    }
}

fn safe_schema_key(value: &str) -> Option<&str> {
    const MAX_KEY_BYTES: usize = 64;
    (!value.is_empty()
        && value.len() <= MAX_KEY_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    .then_some(value)
}

fn is_allowlisted_enum_field(value: &str) -> bool {
    matches!(value, "action" | "audience" | "status" | "type")
}

fn safe_enum_value(value: &str) -> Option<&str> {
    const MAX_ENUM_BYTES: usize = 32;
    (!value.is_empty()
        && value.len() <= MAX_ENUM_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    .then_some(value)
}
