# Working in this repository

Tinkhaven Typing is a web port of Klavaro, published **publicly** at
`github.com/tinkhaven/typing` under GPL-3.0-or-later.

Read [README.md](README.md) for the architecture and
[docs/DIVERGENCE.md](docs/DIVERGENCE.md) for how this port differs from upstream.

---

## This repository is public. Treat every write as a publication.

**Never write into a tracked file:**

- credentials of any kind — keys, tokens, passwords, connection strings
- AWS account IDs, ARNs containing an account ID, access key IDs
- specific AWS profile names, bucket names, cluster names, or role names
- domains, hostnames, or IPs belonging to a real deployment
- local filesystem paths (`/Users/...`, `/home/...`)
- anybody's email address other than the one already in `Cargo.toml`

Deployment-specific values belong in `infra/terraform.tfvars` (gitignored) or in
the operator's shell environment. Terraform variables for such values must have
**no default** — a default in a public repository is a published value.

**Before finishing any task that changed files:**

```bash
make check      # secret scan, formatting, clippy, tests
```

If you added a commit, the hooks ran the scan already — provided
`make hooks` has been run in this clone. Verify with
`git config core.hooksPath` and run `make hooks` if it is unset.

**Never** use `git commit --no-verify` or `git push --no-verify`. If a hook
blocks you, the hook is right until proven otherwise. If it is genuinely a false
positive, add a narrow regex to `.secretsallow` with a comment explaining why —
never widen a rule in `scripts/check-secrets.sh` to silence one line.

If a real secret has already been committed: say so immediately and plainly,
tell the user to rotate the credential first, and do not attempt a history
rewrite on your own initiative.

---

## Conventions

**Comments explain why, never what.** The codebase is full of decisions that
look arbitrary without their reason — why fluidness works in `√(1/Δt)`, why
tasks sit in public subnets, why the wasm-bindgen version is pinned to the CLI.
Preserve that. A comment restating the code is worse than none.

**Cite upstream when porting behaviour.** Reference the Klavaro file and line
(`src/tutor.c:1011`), as the existing code does. It is how the next person
checks the port is faithful.

**Divergences get documented.** Any deliberate difference from Klavaro goes in
`docs/DIVERGENCE.md`. Silent divergence is the failure mode that makes a port
untrustworthy.

**Tests pin behaviour, not implementation.** Name them after the property they
establish (`a_wrong_key_counts_differently_in_each_mode`), and assert on the
thing that would actually break. Every scoring formula and generator has tests
because those are what make the port comparable to upstream — keep it that way.

**Keep `crates/core` free of I/O.** No network, no filesystem, no `web-sys`, no
Leptos. It compiles to both wasm and native, and that is what lets the client and
server agree on what a keystroke means. Parsers take `&str`; callers do the
reading.

---

## The one architectural rule

**The typing loop runs in the browser, and keystroke evaluation never crosses
the network.** Fluidness *is* the variance of the gaps between keystrokes, so
routing keystrokes through the server would measure the connection instead of
the typist. The socket carries a seed and batched *outcomes*, nothing more.

If a change would put a keystroke round-trip in the hot path, it is wrong, no
matter how convenient.

---

## Commands

```bash
make                 # list everything
make hooks           # once per clone: install the secret-scanning hooks
make serve           # run locally on http://localhost:8080
make check           # what to run before pushing
make secrets-history # scan every blob in every commit
make smoke           # drive the practice socket end to end
```

Building needs `cargo-leptos` and a `wasm-bindgen` CLI **whose version matches
the crate in `Cargo.lock`** — the schema is unstable between patch releases.
`make tools` installs both at the pinned versions. Bump them together or not at
all.

`cargo build` works, but `cargo leptos build` is the real build: it produces the
wasm client alongside the server.

---

## Things that will trip you up

- `cargo check` passes where `cargo build` fails on the deeply nested Leptos
  view types. Both crates set `#![recursion_limit = "512"]` for this. Do not
  remove it.
- The release profile is split: `release` for the server (unwinding, so one
  panicking connection does not kill the container) and `wasm-release` for the
  client (size-optimised, `panic = "abort"`). Do not merge them.
- 10 of the 77 `.kbd` files have short rows because trailing spaces were
  stripped upstream. The parser pads; it must not reject them.
- Klavaro's lessons are grouped by keyboard region, not cumulative. Lesson 43 is
  symbols, not "everything".
- **Never put `>` or `>=` in a `view!` attribute expression.** The macro scans
  for `>` to find the end of the tag, so `disabled=move || n >= MAX` closes the
  `<button>` early and the rest of your Rust becomes visible text on the page —
  no warning, no error. Hoist the comparison into a `Memo` and pass that.
