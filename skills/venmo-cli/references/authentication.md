# Authentication and human handoff

Authentication is deliberately interactive. The agent may check status and explain the process, but
the user must enter every login value, including a login SMS code, directly into their own terminal.

## Check status

Run this only when the user asks for authorization status or when diagnosing login or credential
storage:

```sh
venmo auth status
```

The command validates the stored credential with Venmo and identifies the active account. It is not
a prerequisite check for ordinary account commands; those commands validate authorization and
report an error themselves. Do not treat the mere presence of a local credential as proof that it
remains valid.

## Human-only login

Tell the user to run:

```sh
venmo auth login
```

Do not run this command through an agent terminal. Do not ask the user to send any prompted value.
The command interactively requests the account identifier, password, and trusted browser `v_id` or
device ID. It requests a masked six-digit SMS code only if Venmo challenges the login. Password,
device ID, and SMS code prompts display one `*` per entered character and clear the masked prompt
after submission or cancellation; the underlying value is never echoed.

### Retrieve the trusted browser `v_id`

If the user needs help locating it, explain these steps without asking them to share the value:

1. Open [Venmo's account site](https://account.venmo.com/) in a normal browser window.
2. Sign in, complete any MFA challenge, and choose to remember or trust the device if prompted.
3. After the account page loads, open the browser developer tools and select
   **Application/Storage → Cookies**.
4. Select the `account.venmo.com` or `venmo.com` origin.
5. Find the cookie named `v_id` and copy only its value.
6. Paste it directly into the CLI's `Trusted Venmo v_id/device ID` prompt.

The cookie can exist before browser authentication, but it is not trusted until the browser login
and device-trust flow completes. The user should retrieve it only after signing in.

The password, `v_id`, OTP, and bearer token must stay out of chat, command-line arguments,
environment variables, source files, logs, and screenshots. The CLI's interactive prompts keep the
sensitive values out of its command line.

### Credential replacement and storage

- A validated login may replace an existing local credential.
- A failure before the new credential is stored leaves the existing local entry untouched.
- The CLI uses the platform keyring when one is available.
- On Linux, a missing keyring can cause the CLI to offer an XDG credential fallback. The user must
  review and explicitly accept any plaintext-storage warning in their own terminal.
- If login reports incomplete device trust or credential cleanup, follow the printed recovery
  guidance and verify with `venmo auth status` before doing anything else.

After the user says login is complete, resume the original account command. It validates the new
credential itself. Use `venmo auth status` separately only when diagnosis is needed.

## Logout

`auth logout` removes locally stored authorization:

```sh
venmo auth logout
```

It does not contact Venmo and does not revoke the remote bearer token. Before running it for the
user, explain that distinction and get explicit confirmation because there is no `--dry-run` flag.
If remote revocation is required, direct the user to Venmo's official session or security controls.
