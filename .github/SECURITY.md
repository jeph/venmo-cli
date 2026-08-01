# Security Policy

## Supported versions

venmo-cli is currently alpha software built on reverse-engineered, non-public Venmo endpoints. Only
the latest published release receives security fixes.

| Version | Security updates |
| --- | --- |
| Latest release | Supported |
| Older releases | Not supported |
| Unreleased commits | Not supported |

Before reporting a problem, reproduce it with the
[latest release](https://github.com/jeph/venmo-cli/releases/latest) when it is safe to do so.

## Reporting a vulnerability

Do not disclose suspected vulnerabilities in a public issue, discussion, pull request, or social
media post. Submit a
[private vulnerability report](https://github.com/jeph/venmo-cli/security/advisories/new) instead.

Include enough information to investigate without exposing private data:

- The affected venmo-cli version, operating system, and CPU architecture.
- The security impact and who could exploit it.
- Minimal reproduction steps or a proof of concept.
- Sanitized command output or debug diagnostics, if relevant.
- Any suggested mitigation or fix.

Never include bearer tokens, `v_id` cookies, OTP codes, keyring contents, bank or card details,
transaction identifiers, usernames, payment notes, amounts, or other personal or financial data.
The maintainer will not ask you to provide authentication secrets.

Reports will be assessed privately. If the report is accepted, the maintainer will coordinate a fix,
a release, and public disclosure as appropriate. Please allow time to investigate before publishing
details. Reporters can be credited in a published advisory if they want attribution.

## In scope

Examples of security issues in venmo-cli include:

- Exposure or insecure storage of Venmo credentials or device identifiers.
- Sensitive data appearing in normal output, JSON output, errors, or debug logs.
- A way to bypass confirmation, `--dry-run`, funding-source selection, visibility, or other mutation
  safety controls.
- Incorrect outcome handling that could make a financial mutation unsafe to retry.
- Authentication or OTP handling that weakens account security.
- Release artifact, signing, update, or packaging issues that could allow code substitution.
- Local privilege or file-permission flaws caused by venmo-cli.

## Out of scope

The following are not vulnerabilities in this project by themselves:

- A non-public Venmo endpoint changing, disappearing, or rejecting a request.
- Venmo service availability, rate limits, account policy, fees, or eligibility decisions.
- Social engineering or phishing that does not exploit venmo-cli.
- Vulnerabilities that require a user to intentionally provide an attacker with credentials or
  unrestricted access to an already unlocked account.

Vulnerabilities in Venmo or PayPal systems are outside this project's control and should be reported
through their official security process. You may also notify this project privately when a CLI-side
mitigation could protect venmo-cli users.

## Safe research

- Test only with accounts and data you own or have explicit permission to use.
- Prefer `--dry-run` and non-mutating commands. Do not complete a real payment or transfer solely to
  demonstrate a vulnerability.
- Do not access, retain, or disclose another person's data.
- Do not disrupt Venmo, PayPal, this repository, or other users.
- Stop testing if you encounter data or access outside your authorization.
