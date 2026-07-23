# Friends JSON shapes

These examples show command-specific `data` objects used with the shared JSON contract in
`SKILL.md`. Values are illustrative.

## `friends list`

Expected envelope command: `friends.list`.

```json
{
  "subject": null,
  "users": [
    {
      "user_id": "123456789",
      "username": "alice",
      "display_name": "Alice Example",
      "profile_kind": "personal",
      "is_payable": true,
      "friendship_status": "friend"
    }
  ],
  "next_offset": null
}
```

`subject` is null for the active account. With `--user`, it contains `user_id` and `username` for
the resolved profile. `next_offset` is either an integer continuation or null. User fields other
than `user_id` can be null.

## `friends add`

Expected envelope command: `friends.add`.

```json
{
  "outcome": "completed",
  "performed": true,
  "plan": {
    "account": {
      "user_id": "987654321",
      "username": "current_user",
      "display_name": "Current User"
    },
    "target": {
      "user_id": "123456789",
      "username": "alice",
      "display_name": "Alice Example",
      "profile_kind": "personal",
      "is_payable": true,
      "friendship_status": "not_friend"
    },
    "previous_status": "not_friend",
    "action": "send_request",
    "automatic_retries": false
  },
  "result": {
    "accepted": true,
    "action": "send_request"
  }
}
```

`previous_status` is `not_friend` for `send_request` and `request_received` for `accept_request`.
The target always has a username, profile kind `personal`, and that friendship status; display name
and payability can be null. `result.accepted` means Venmo accepted the operation, not that the target
accepted a sent request.

## `friends remove`

Expected envelope command: `friends.remove`.

```json
{
  "outcome": "completed",
  "performed": true,
  "plan": {
    "account": {
      "user_id": "987654321",
      "username": "current_user",
      "display_name": "Current User"
    },
    "target": {
      "user_id": "123456789",
      "username": "alice",
      "display_name": "Alice Example",
      "profile_kind": "personal",
      "is_payable": true,
      "friendship_status": "friend"
    },
    "previous_status": "friend",
    "action": "unfriend",
    "automatic_retries": false
  },
  "result": {
    "accepted": true,
    "action": "unfriend"
  }
}
```

`previous_status` is `friend` for `unfriend` and `request_sent` for `cancel_request`. The target
always has a username, profile kind `personal`, and that friendship status; display name and
payability can be null. `result.accepted` means Venmo accepted the operation.
