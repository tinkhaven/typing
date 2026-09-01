# Tinkhaven Typing

A web port of **[Klavaro](https://klavaro.sourceforge.io/)**, the touch typing
tutor by Felipe Emmanuel Ferreira de Castro — the same four modules, the same 77
keyboard layouts, the same 43 lessons, the same scoring, in a browser.

Rust throughout: [Leptos](https://leptos.dev) compiled to WebAssembly for the
client, Axum for the server, one shared crate holding the parts both agree on.

> Not affiliated with or endorsed by the Klavaro project. Please report problems
> with this port here rather than upstream. See [CREDITS.md](CREDITS.md).

**Status: early.** All four modules work and the scoring matches upstream, but
this is young code and rough in places. Progress charts over time are the main
Klavaro feature not yet ported; [docs/DIVERGENCE.md](docs/DIVERGENCE.md) lists
everything else that differs or is missing.

## The four modules

| Module | What it does | Goal |
|---|---|---|
| **Basic course** | 43 lessons introducing the keys a few at a time | 95% accuracy, 10 wpm |
| **Adaptability** | Invented words spanning the whole layout, so you cannot guess ahead | 98% accuracy, 10 wpm |
| **Velocity** | Real words from a language corpus, no punctuation | 95% accuracy, 50 wpm |
| **Fluidness** | Real prose; mistakes must be corrected before moving on | 97% accuracy, 50 wpm, 70% fluidness |

Basic, Adaptability and Velocity never let you back up — a wrong key is marked
and the cursor moves on. Fluidness makes you backspace over a mistake and retype
it, and charges the position once when you do.

## How the numbers work

Taken verbatim from `src/tutor.c:1011`–`1047` upstream:

```
accuracy  = 100 · (1 − errors / keystrokes)
speed     = 12 · (keystrokes − errors) / seconds        12 = 60s ÷ 5 chars/word
fluidness = 100 · (1 − σ/μ)   over samples sᵢ = √(1/Δtᵢ), skipping the first two
```

Fluidness is the interesting one. Working in `√(1/Δt)` rather than `Δt` compresses
the long tail of pauses, so it rewards a steady pace instead of punishing the
occasional hesitation — two pauses in forty keystrokes barely move it, while a
consistently erratic rhythm does.

## Design

**The typing loop runs entirely in the browser.** This is not an optimisation, it
is a correctness requirement: fluidness *is* the variance of the gaps between
keystrokes, so measuring it across a network would measure the network. The
client generates its exercise from a random seed and evaluates every keystroke
locally.

**The WebSocket is for the server relationship, not the keystrokes.** The client
tells the server which seed it used, the server regenerates the same text, and
the client then streams batched keystroke outcomes. The server keeps its own
tally and scores the run from *that*, so a leaderboard entry is not simply a
number the browser reported. Pull the plug mid-exercise and the typist notices
nothing — the run just cannot be published.

**No accounts, no cookies.** Preferences live in `localStorage`. Nothing is
recorded about a visitor unless they publish a score, and then only a nickname
they typed, three numbers and a date — deleted automatically after a year.

```
crates/core   pure logic, no I/O — layouts, lessons, generators, scoring,
              the typing state machine. Compiles to both wasm and native.
crates/web    Leptos client (feature `hydrate`) + Axum server (feature `ssr`),
              with src/protocol.rs as the seam between them.
assets/       Klavaro's data files, used verbatim
infra/        Terraform: ECR, Fargate, ALB, ACM, Route 53, DynamoDB
```

## Running it locally

```bash
cargo leptos serve
```

Then open <http://localhost:8080>. You will need:

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked cargo-leptos --version 0.3.7
cargo install --locked wasm-bindgen-cli --version 0.2.127
```

Those two versions go together. The wasm-bindgen schema is unstable between patch
releases, and cargo-leptos drives whichever `wasm-bindgen` is on your `PATH`, so a
mismatch fails the build with a schema error. If you bump one, bump the other.

Without `LEADERBOARD_TABLE` set, the leaderboard is kept in memory and says so in
the log — fine for local work, lost on restart.

## Tests

```bash
cargo test --workspace                                    # core logic
cargo test -p typing-web --no-default-features --features ssr
python3 tests/ws_smoke.py                                  # against a running server
```

The core crate's tests are where the port is actually pinned down: the scoring
formulas, the lesson curriculum, the exercise generators, and the two keystroke
evaluation modes. `tests/ws_smoke.py` drives a real socket end to end — an honest
run, publishing, and every rejection path.

## Deploying

Set up once:

```bash
export AWS_PROFILE=<your-admin-profile>
cp infra/terraform.tfvars.example infra/terraform.tfvars   # set your domain
terraform -chdir=infra init
terraform -chdir=infra apply
```

`infra/terraform.tfvars` is gitignored. The domain and hosted zone have no
defaults on purpose: deployment-specific values do not belong in a public
repository, and a stale default pointed at someone else's domain is worse than
no default.

Then each deploy:

```bash
./deploy.sh
```

That builds an ARM64 image, pushes it to ECR and rolls the ECS service. ARM64
because Graviton Fargate is about 20% cheaper and builds natively on an Apple
Silicon Mac; set `cpu_architecture = "X86_64"` in `infra/terraform.tfvars` if you
would rather build for Intel.

Running cost is roughly **$28/month**, of which the load balancer is about $18.
Tasks run in public subnets on purpose — private subnets would need a NAT
gateway, which costs more than everything else here combined — with a security
group that only admits traffic from the load balancer.

One thing to know before scaling past a single task: live leaderboard pushes are
broadcast within a task, so with several tasks a visitor sees instant updates
only from the one they happen to be connected to. The board itself stays correct,
since it is read from DynamoDB.

## Licence

GPL-3.0-or-later, because this is a derivative work of Klavaro. The browser runs
compiled WebAssembly, which is a distribution of object code, so the
corresponding source is this repository. See [LICENSE](LICENSE),
[CREDITS.md](CREDITS.md) for attribution and provenance, and
[docs/DIVERGENCE.md](docs/DIVERGENCE.md) for every place this port deliberately
differs from upstream.
