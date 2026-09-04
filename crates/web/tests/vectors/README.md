# Test vectors

Genuinely RS256-signed JWTs and the **public** key that verifies them, used by
`server::jwt` and `tests/google_signin.rs`.

Generated once with OpenSSL. The private key was discarded immediately and is
not here: these files contain a public modulus and a set of signed tokens, none
of which is a credential and none of which grants anything. The alternative —
generating a key at test time — would mean a dependency on the `rsa` crate,
which carries RUSTSEC-2023-0071 with no fix available, and `cargo audit` reads
dev-dependencies too.

`valid.jwt` expires in 2100 so the vector does not rot. The others differ from
it in exactly one respect each, named by the filename, so a failing test says
which check stopped working.
