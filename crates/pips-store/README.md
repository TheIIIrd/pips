# pips-store

Backups of a Pips collection: every card with its place (collection,
archive, bin), arrival time, sender, edition certificate and note, plus the
profile. Not the secret key — the 24 words are its backup.

Encrypted to the owner: XChaCha20-Poly1305 with a key derived from the
identity's seed, so a backup opens wherever the same words were entered
and nowhere else, with no password to forget. The owner's public key is in
the clear, so a wrong-key error can say whose backup it is. Layout and
rules: see the crate documentation.

Strict like every Pips parser: lengths checked before use, reserved bits
zero, no trailing bytes, every card signature and certificate verified,
"own" cards must be signed by the owner. `seal` decodes what it is about to
write, so a file it produces always opens.

```sh
pips-backup owner pips-2026-10-20.pipsbak
pips-backup show -k me.key pips-2026-10-20.pipsbak
```

Tests (`tests/backup.rs`): round trip, restore through the 24 words, wrong
key, no plaintext in the file, every single flipped bit of a file refused,
every truncation refused, strict body rules, a huge entry count refused
before allocation.
