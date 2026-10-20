# pips-sign

Identity and trust for Pips cards. `no_std` + `alloc`, no `unsafe`; built on
`ed25519-dalek` and `bip39`. Never touches a random number generator itself:
callers pass a seed from the platform's CSPRNG.

- **Keys** — Ed25519; the secret is a 32-byte seed, wiped from memory on drop.
- **Recovery** — the seed as 24 English BIP39 words (the words *are* the
  seed: no PBKDF2), or as a paper code `PIPS:KEY:1:…` (~55 characters, a
  small QR). Neither is encrypted: keep them like a passport.
- **Card signatures** — over `"pips-v1:card"` and the canonical form, so they
  survive re-encoding. `verify_strict` refuses weak keys and malleable
  signatures.
- **Edition certificates** — 105 bytes: copy N of M (M = 0 for an open run),
  holder's key, day of issue, author's signature over the card's digest.
  Only the author can issue them.
- **Recipient codes** — `'R'`, version, key, nickname: what you show to
  receive a certified copy.
- **Envelopes** — `'E'`, version, flags, card, optional certificate,
  optional signed sender. The reserved expiry flag is refused, not ignored.
  `open` verifies everything or returns nothing.

## Command line

```sh
pips-sign keygen -o me.key                 # 24 words, mode 0600, never overwrites
pips-sign card -k me.key -o signed.pips card.pips
pips-sign verify signed.pips
pips-sign recipient -k friend.key --nick moss > friend.code
pips-sign seal -k me.key --edition 3 --of 50 --to friend.code signed.pips | pips-b45
pips-b45 -d < scanned.txt | pips-sign open
```

## Testing

BIP39 reference vectors; every single-character change to a paper key
caught; every edit to a signed card breaks its signature; certificates bound
to one card and one holder; every bit flip of an envelope either refused or
provably harmless (deflate padding: same content, same signatures);
truncation at every length; the full edition flow through the tool.
