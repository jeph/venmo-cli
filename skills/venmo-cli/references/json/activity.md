# Activity JSON shapes

These are command-specific `data` shapes used with the shared JSON contract in `SKILL.md`. Bare
`string`, `integer`, `boolean`, and `object` values denote JSON types; `|` separates allowed values.

## `activity list`

Expected envelope command: `activity.list`.

```text
{
  "subject": {
    "user_id": string,
    "username": string,
    "kind": "other_personal_user"
  } | null,
  "activities": [
    {
      "id": string,
      "occurred_at": timestamp,
      "action": string,
      "direction": "incoming" | "outgoing",
      "counterparty": {
        "kind": "user",
        "user": {
          "user_id": string,
          "username": string | null,
          "display_name": string | null,
          "profile_kind": "personal" | "business" | "charity" | "unknown" | null,
          "is_payable": boolean | null,
          "friendship_status":
            "friend" | "not_friend" | "request_received" | "request_sent" | null
        }
      },
      "amount": { "amount": string, "currency": "USD" } | null,
      "status": string | null,
      "note": string | null,
      "audience": string | null
    }
  ],
  "next_before_id": string | null
}
```

An external counterparty instead has `kind: "external"`, `name`, `type`, and nullable `last_four`
strings. `subject` is null for the active account. `next_before_id` is the opaque continuation.
Amount and status can be null, especially for other-user feeds and activity types that omit them.

## `activity info`

Expected envelope command: `activity.info`.

```text
{
  "activity": {
    "id": string,
    "occurred_at": timestamp,
    "action": string,
    "parties": {
      "kind": "payment",
      "actor": object,
      "target": object
    },
    "amount": { "amount": string, "currency": "USD" } | null,
    "status": string | null,
    "note": string | null,
    "audience": string | null,
    "social": {
      "likes": {
        "count": integer,
        "items": [object],
        "complete": boolean
      } | null,
      "comments": {
        "count": integer,
        "items": [
          {
            "id": string,
            "author": object,
            "message": string,
            "created_at": timestamp
          }
        ],
        "complete": boolean
      } | null,
      "reactions": {
        "count": integer
      } | null
    }
  }
}
```

User objects in `actor`, `target`, authors, and likes have the user fields shown under
`activity list`. Parties can also be `relative` with `direction` and `counterparty`, or `account`
with `account`, `direction`, and `counterparty`. Counterparties have the user or external shape
described above. `complete: false` means embedded `items` are a subset of the reported `count`.

## `activity comments list`

Expected envelope command: `activity.comments.list`.

```text
{
  "activity_id": string,
  "comments": [
    {
      "id": string,
      "author": object,
      "message": string,
      "created_at": timestamp
    }
  ],
  "total_count": integer,
  "offset": integer,
  "next_offset": integer | null
}
```

Each author is a user object. `next_offset` is the continuation for the same activity ID.

## `activity reactions list`

Expected envelope command: `activity.reactions.list`.

```text
{
  "activity_id": string,
  "total_count": integer,
  "reactions": [
    {
      "emoji": string,
      "kind": "unicode_emoji" | "custom_alias",
      "count": integer,
      "reacted_by_current_user": boolean
    }
  ]
}
```

`total_count` is the checked sum of all aggregate reaction counts. `custom_alias` values are emitted
exactly as Venmo returned them and are read-only; add and remove accept `unicode_emoji` values or the
special `like` target. The command does not expose the identities of other reacting users. Venmo
exposes the `❤️` reaction and the like as the same state.

## Comment addition

Expected envelope command: `activity.comments.add`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "activity": object,
    "action": "add_comment",
    "message": string,
    "previous_like_state": "liked" | "not_liked" | "unknown",
    "automatic_retries": false
  },
  "result": { "activity": object } | null
}
```

`plan.activity` and `result.activity` use the inner object at `activity info`'s `data.activity`.

## Reaction mutations

Expected envelope commands are `activity.reactions.add` and `activity.reactions.remove`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "activity": object,
    "action": "add_reaction" | "remove_reaction",
    "target": {
      "kind": "like" | "unicode_emoji",
      "value": string
    },
    "previous_state": "present" | "absent" | "unknown",
    "automatic_retries": false
  },
  "result": {
    "activity_id": string,
    "target": {
      "kind": "like" | "unicode_emoji",
      "value": string
    },
    "state": "present" | "absent",
    "count": integer | null,
    "reacted_by_current_user": boolean | null
  } | null
}
```

Exact lowercase `like` uses the bodyless likes endpoint. Unicode emoji, including literal `❤️`, use
the reactions endpoint. Venmo exposes `like` and `❤️` as the same server state even though these
inputs select different endpoints.

## `activity comments remove`

Expected envelope command: `activity.comments.remove`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "comment_id": string,
    "preflight_scope": "comment_id_only",
    "verification_required": true,
    "automatic_retries": false
  },
  "result": {
    "accepted": true,
    "verification_required": true
  } | null
}
```

Acceptance is not authoritative absence; the command marks verification as required.
