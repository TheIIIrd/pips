# Security

Pips keeps an identity key on the device and checks signatures on
everything it receives, so flaws in parsing or cryptography matter.

Please report vulnerabilities privately through GitHub: **Security →
Report a vulnerability** on this repository. Do not open a public issue.

In scope: the crates in `crates/` (parsers, signatures, certificates,
envelopes, backups) and the app in `web/` (key storage, CSP, anything a
received code or file can make it do).
