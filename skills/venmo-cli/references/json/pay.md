# Payment JSON shapes

These examples show command-specific `data` objects used with the shared JSON contract in
`SKILL.md`. Values are illustrative.

## `pay options`

Expected envelope command: `pay.options`.

```json
{
  "payment_methods": [
    {
      "id": "payment-method-id",
      "name": "Example Bank",
      "type": "bank",
      "last_four": "1234",
      "is_default": true
    }
  ]
}
```

`name`, `type`, and `last_four` can be null. `id` is the opaque payment-method identifier.

## `pay user`

Expected envelope command: `pay.user`.

```json
{
  "outcome": "completed",
  "performed": true,
  "plan": {
    "client_request_id": "generated-request-id",
    "account": {
      "user_id": "987654321",
      "username": "current_user",
      "display_name": "Current User"
    },
    "recipient": {
      "user_id": "123456789",
      "username": "alice",
      "display_name": "Alice Example",
      "profile_kind": "personal",
      "is_payable": true,
      "friendship_status": "friend"
    },
    "amount": {
      "amount": "12.34",
      "currency": "USD"
    },
    "note": "Dinner",
    "visibility": "private",
    "balance": {
      "available": {
        "amount": "50.00",
        "currency": "USD"
      },
      "on_hold": {
        "amount": "0.00",
        "currency": "USD"
      }
    },
    "funding_source": {
      "kind": "external",
      "method": {
        "id": "payment-method-id",
        "name": "Example Bank",
        "type": "bank",
        "last_four": "1234",
        "is_default": true
      },
      "role": "default",
      "fee": {
        "status": "known",
        "amount": {
          "amount": "0.00",
          "currency": "USD"
        }
      }
    },
    "funding_source_selection": "automatic",
    "eligibility_fee": {
      "amount": "0.00",
      "currency": "USD"
    },
    "purchase_protected": false,
    "purchase_protection_fee": null,
    "recipient_proceeds": null,
    "automatic_retries": false
  },
  "result": {
    "id": "payment-id",
    "status": "settled",
    "purchase_protected": false
  }
}
```

Account and recipient display names can be null; the recipient always has a username, profile kind
`personal`, and `is_payable: true`. Friendship status can be null. Visibility is `private`, `friends`,
or `public`. Funding source selection is `automatic` or `explicit`. A funding source is `balance` or
`external`; balance sources have null `role` and `fee`. External roles are `default` or `backup`. A
fee is `known` with a money object or `unknown` with a null amount. Protection fee and proceeds can
be null. Result status is `settled`, `pending`, or `held`.
