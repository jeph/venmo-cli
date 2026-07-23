# Balance JSON shape

This example shows the command-specific `data` object used with the shared JSON contract in
`SKILL.md`. Values are illustrative.

## `balance`

Expected envelope command: `balance`.

```json
{
  "balance": {
    "available": {
      "amount": "12.34",
      "currency": "USD"
    },
    "on_hold": {
      "amount": "0.00",
      "currency": "USD"
    }
  }
}
```

Both amounts are exact decimal strings and can be negative. `available` and `on_hold` are separate
values; the response does not include their sum.
