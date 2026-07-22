# Authentication and human handoff

Authentication is deliberately interactive. The agent may check status and explain the process, but
the user must enter every login and SMS verification value directly into their own terminal.

## Check status

Run this before account operations and again after any login handoff:

```sh
venmo auth status
```

The command validates the stored credential with Venmo and identifies the active account. Do not
treat the mere presence of a local credential as proof that it remains valid.

## Human-only login

Tell the user to run:

```sh
venmo auth login
```

Do not run this command through an agent terminal. Do not ask the user to send any prompted value.
The command interactively requests the account identifier, password, and trusted browser `v_id` or
device ID. It requests a hidden six-digit SMS code only if Venmo challenges the login.

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

After the user says login is complete, run `venmo auth status`. Continue only after it validates the
credential and displays the intended active account.

## SMS verification during payments and requests

Payments, request creation, and request acceptance can require P2P SMS verification. A `--dry-run`
finishes before any OTP prompt or write. During actual execution, the first Venmo response may say
that step-up verification is required.

If the agent's command environment is not interactive, the CLI reports:

```text
Venmo requires SMS verification; rerun in a terminal that can prompt for the code
```

At that point:

1. Do not ask for the SMS code.
2. Do not attempt to pipe a code into the command.
3. Give the user the exact approved command with `--yes` removed.
4. Ask the user to run it in their own terminal, review the default-No confirmation, and enter the
   hidden OTP there if requested.
5. Afterward, use the appropriate read command to verify the resulting state.

## Logout

`auth logout` removes locally stored authorization:

```sh
venmo auth logout
```

It does not contact Venmo and does not revoke the remote bearer token. Before running it for the
user, explain that distinction and get explicit confirmation because there is no `--dry-run` flag.
If remote revocation is required, direct the user to Venmo's official session or security controls.
