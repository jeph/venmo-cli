# Transfer JSON shapes

These are command-specific `data` shapes used with the shared JSON contract in `SKILL.md`. Bare
`string`, `integer`, `boolean`, and `object` values denote JSON types; `|` separates allowed values.

## `transfer options`

Expected envelope command: `transfer.options`.

```text
{
  "preferred_out": "standard" | "instant" | null,
  "standard": {
    "eligible_destinations": [
      {
        "id": string,
        "name": string,
        "asset_name": string,
        "type": string,
        "last_four": string,
        "is_default": boolean,
        "estimated_completion": string
      }
    ],
    "fee": {
      "minimum_amount": string | null,
      "maximum_amount": string | null,
      "variable_percentage": string | null,
      "has_additional_fields": boolean
    },
    "estimated_completion": string
  },
  "instant": {
    "eligible_destinations": [object],
    "fee": {
      "minimum_amount": string | null,
      "maximum_amount": string | null,
      "variable_percentage": string | null,
      "has_additional_fields": boolean
    },
    "estimated_completion": string
  }
}
```

Instant destinations have the same shape as standard destinations. Fee fields are strings supplied
by Venmo rather than normalized money or numeric values. An empty destination array means that mode
has no eligible destination. The current transfer-out command supports only standard mode.

## `transfer out`

Expected envelope command: `transfer.out`.

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
    "balance": {
      "available": { "amount": string, "currency": "USD" },
      "on_hold": { "amount": string, "currency": "USD" }
    },
    "amount_selection": {
      "kind": "exact",
      "amount": { "amount": string, "currency": "USD" }
    },
    "amount": { "amount": string, "currency": "USD" },
    "speed": "standard",
    "destination": {
      "id": string,
      "name": string,
      "asset_name": string,
      "type": "bank",
      "last_four": string,
      "is_default": boolean,
      "estimated_completion": string
    },
    "automatic_retries": false
  },
  "result": {
    "id": string,
    "status": "pending",
    "requested_at": timestamp,
    "net_amount": { "amount": string, "currency": "USD" },
    "fee": { "amount": string, "currency": "USD" }
  } | null
}
```

For `all`, `amount_selection.kind` is `all_available` and its amount is null; `plan.amount` is the
resolved amount. The completed net amount can differ from the plan amount when a fee applies.
