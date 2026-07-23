# Auth JSON shapes

These examples show command-specific `data` objects used with the shared JSON contract in
`SKILL.md`. Values are illustrative. Authentication values remain private, and `auth login` is
always human-operated.

## `auth status`

Expected envelope command: `auth.status`.

```json
{
  "account": {
    "user_id": "123456789",
    "username": "alice",
    "display_name": "Alice Example"
  },
  "saved_at": "2026-07-22T12:34:56Z",
  "credential_format": "version_1",
  "credential_backend": "keyring"
}
```

`display_name` can be null. `credential_format` is `version_1` or `legacy_typescript`;
`credential_backend` is `keyring` or `xdg`.

## `auth login`

Expected envelope command: `auth.login`. This command is human-operated rather than automated.

```json
{
  "credential_stored": true,
  "account": {
    "user_id": "123456789",
    "username": "alice",
    "display_name": "Alice Example"
  },
  "saved_at": "2026-07-22T12:34:56Z",
  "disposition": "created",
  "device_trust": {
    "status": "trusted",
    "message": null
  },
  "previous_remote_token_revoked": null
}
```

`disposition` is `created`, `replaced_existing_credential`, or `recovered_unusable_entry`.
`device_trust.status` is `not_needed`, `trusted`, or `failed`; its message can be null.
`previous_remote_token_revoked` is false or null.

An incomplete login can return this same object as failure `partial_result`, commonly with
`credential_stored: true` and `device_trust.status: "failed"`. Treat it according to the failure
envelope; verify with `auth status` rather than treating the partial result as full success.

## `auth logout`

Expected envelope command: `auth.logout`.

```json
{
  "local_credential": {
    "status": "deleted",
    "message": null
  },
  "remote_token_revoked": false
}
```

`local_credential.status` is `deleted`, `missing`, or `failed`; its message can be null. The command
has no dry-run. A local failure or partially completed cleanup can use a failure envelope, so
inspect `error.outcome` and `partial_result` rather than relying on process text alone.
