# Security

## Reporting something

Open a [private security advisory](https://github.com/tinkhaven/typing/security/advisories/new)
rather than a public issue. If that is not available to you, a normal issue
saying only "I have found a security problem, how should I send it?" is fine —
no details in the open, please.

This is a typing tutor with no accounts and no personal data, so the realistic
severity ceiling is low. Report it anyway; a leaked credential or a way to
poison the leaderboard is still worth fixing quickly.

## What this repository never contains

No credentials, keys, tokens, account identifiers, or deployment-specific
configuration. Anything of that shape belongs in the operator's environment or
in an untracked file. This is enforced rather than hoped for:

| Layer | What it does | Where |
|---|---|---|
| `pre-commit` hook | Scans staged changes | `.githooks/pre-commit` |
| `pre-push` hook | Scans every commit about to become public | `.githooks/pre-push` |
| CI, before anything else | Scans every blob in every commit, plus gitleaks | `.github/workflows/ci.yml` |
| CI, weekly | Full history rescan and dependency advisories | `.github/workflows/security.yml` |
| GitHub push protection | Blocks a leaking push server-side, even with hooks bypassed | Repository settings |

Install the hooks after cloning:

```bash
make hooks
```

Run the checks by hand any time:

```bash
make secrets            # every tracked file
make secrets-history    # every blob in every commit, ever
make check              # secrets, formatting, clippy, tests
```

The hooks are only advisory — `--no-verify` skips them, and a clone without
`make hooks` has none. GitHub's push protection is the backstop that does not
depend on anyone's local setup.

### If something does leak

Deleting the line is not a fix. Anything pushed to a public repository should be
treated as compromised the moment it lands: it is in clones, in forks, and in
whatever scrapes GitHub's event firehose.

1. **Rotate the credential first.** Before cleaning up, before telling anyone.
2. Remove it from history (`git filter-repo`), force-push, and ask GitHub
   Support to purge the cached blobs.
3. Add a rule to `scripts/check-secrets.sh` so that shape cannot recur.

## What the hosted service stores

No accounts, no cookies, no analytics. Preferences live in the visitor's own
`localStorage`. Nothing is recorded server-side unless someone chooses to publish
a score, and then only a nickname they typed, three numbers and a date — no
email, no IP address — deleted automatically after a year.

Keystroke outcomes are streamed to the server while practising, so it can score a
run rather than trust the browser's arithmetic. They are not stored.

## Deliberate limitations

**The leaderboard is not cheat-proof.** The server keeps its own tally, so a
browser cannot simply post a number, and `crates/web/src/server/verify.rs`
rejects results that could not have come from the exercise issued. A determined
person can still synthesise a plausible keystroke stream. Hardening that further
would need accounts, which this project does not want. It is a typing tutor.

**Tasks run in public subnets.** Private subnets would need a NAT gateway, which
costs more than everything else in the deployment combined. Inbound is restricted
to the load balancer's security group, so the container is not reachable from the
internet. See the comments in `infra/main.tf`.
