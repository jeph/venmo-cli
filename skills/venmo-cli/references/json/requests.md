# Request JSON shapes

These are command-specific `data` shapes used with the shared JSON contract in `SKILL.md`. Bare
`string`, `integer`, `boolean`, and `object` values denote JSON types; `|` separates allowed values.

## `requests list`

Expected envelope command: `requests.list`.

```text
{
  "direction": "all" | "incoming" | "outgoing",
  "requests": [
    {
      "id": string,
      "action": "charge",
      "direction": "incoming" | "outgoing",
      "counterparty": {
        "user_id": string,
        "username": string | null,
        "display_name": string | null,
        "profile_kind": "personal" | "business" | "charity" | "unknown" | null,
        "is_payable": boolean | null,
        "friendship_status":
          "friend" | "not_friend" | "request_received" | "request_sent" | null
      },
      "amount": { "amount": string, "currency": "USD" },
      "note": string | null,
      "audience": string | null,
      "created_at": timestamp | null,
      "status": "pending" | "held"
    }
  ],
  "next_before": string | null
}
```

`next_before` is the opaque continuation. The server page is direction-filtered locally, so the
array can contain fewer records than the requested limit even when a continuation exists.

## `requests info`

Expected envelope command: `requests.info`.

```text
{
  "request": {
    "id": string,
    "action": "charge",
    "direction": "incoming" | "outgoing",
    "counterparty": object,
    "amount": { "amount": string, "currency": "USD" },
    "note": string | null,
    "audience": string | null,
    "created_at": timestamp | null,
    "status": "pending" | "held"
  }
}
```

The counterparty has the user shape shown under `requests list`.

## `requests create`

Expected envelope command: `requests.create`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "client_request_id": string,
    "account": {
      "user_id": string,
      "username": string,
      "display_name": string | null
    },
    "recipient": {
      "user_id": string,
      "username": string,
      "display_name": string | null,
      "profile_kind": "personal",
      "is_payable": true,
      "friendship_status":
        "friend" | "not_friend" | "request_received" | "request_sent" | null
    },
    "amount": { "amount": string, "currency": "USD" },
    "note": string,
    "visibility": "private" | "friends" | "public",
    "automatic_retries": false
  },
  "result": {
    "id": string,
    "status": "pending"
  } | null
}
```

The recipient is a resolved payable personal profile.

## `requests accept`

Expected envelope command: `requests.accept`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "account": {
      "user_id": string,
      "username": string,
      "display_name": string | null
    },
    "request": object,
    "balance": {
      "available": { "amount": string, "currency": "USD" },
      "on_hold": { "amount": string, "currency": "USD" }
    },
    "funding_source": {
      "kind": "balance" | "external",
      "method": {
        "id": string,
        "name": string | null,
        "type": string | null,
        "last_four": string | null,
        "is_default": boolean
      },
      "role": "default" | "backup" | null,
      "fee": {
        "status": "known" | "unknown",
        "amount": { "amount": string, "currency": "USD" } | null
      } | null
    },
    "funding_source_selection": "automatic" | "explicit",
    "purchase_protected": boolean,
    "purchase_protection_fee": { "amount": string, "currency": "USD" } | null,
    "recipient_proceeds": { "amount": string, "currency": "USD" } | null,
    "automatic_retries": false
  },
  "result": {
    "request_id": string,
    "payment_id": string | null,
    "status": "settled" | "pending" | "held" | null
  } | null
}
```

`plan.request` is the inner object at `requests info`'s `data.request`, with action `charge`,
direction `incoming`, status `pending`, audience `private`, and non-null note, created time, and
requester username. Its requester has profile kind `personal` and `is_payable: true`. A balance
funding source has null `role` and `fee`; an external fee is `known` with a money object or `unknown`
with a null amount.

## `requests decline`

Expected envelope command: `requests.decline`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "account": object,
    "request": object,
    "action": "decline",
    "money_sent": false,
    "automatic_retries": false
  },
  "result": {
    "request_id": string,
    "status": "cancelled",
    "money_sent": false
  } | null
}
```

The account has the fields shown under `requests accept`. `plan.request` is the inner object at
`requests info`'s `data.request`, with action `charge`, direction `incoming`, status `pending`, and
non-null note, created time, and requester username. Audience is `private`, `friends`, or `public`.

## `requests cancel`

Expected envelope command: `requests.cancel`.

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": {
    "account": object,
    "request": object,
    "action": "cancel",
    "money_sent": false,
    "automatic_retries": false
  },
  "result": {
    "request_id": string,
    "status": "cancelled",
    "money_sent": false
  } | null
}
```

The account has the fields shown under `requests accept`. `plan.request` is the inner object at
`requests info`'s `data.request`, with action `charge`, direction `outgoing`, status `pending` or
`held`, and non-null note, created time, and recipient username. Audience is `private`, `friends`, or
`public`.
