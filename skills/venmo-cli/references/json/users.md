# Users JSON shapes

These examples show command-specific `data` objects used with the shared JSON contract in
`SKILL.md`. Values are illustrative.

## `users search`

Expected envelope command: `users.search`.

```json
{
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

`next_offset` is the exact value accepted by the next `--offset`; it is not derived from the current
offset or array length.

## `users info`

Expected envelope command: `users.info`.

```json
{
  "user": {
    "user_id": "123456789",
    "username": "alice",
    "display_name": "Alice Example",
    "profile_kind": "personal",
    "is_payable": true,
    "friendship_status": "friend"
  }
}
```

`username` is present. `display_name`, `profile_kind`, `is_payable`, and `friendship_status` can be
null. Non-null profile kinds are `personal`, `business`, `charity`, or `unknown`; friendship statuses
are `friend`, `not_friend`, `request_received`, or `request_sent`.
