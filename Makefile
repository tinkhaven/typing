# Tinkhaven Typing — one entry point for the things worth doing.
#
#   make            list the targets
#   make check      what to run before you push (this is the important one)
#   make hooks      install the versioned git hooks (do this once, after cloning)
#
# `make check` is deliberately the same set of checks that CI runs, so a green
# local run means a green pipeline.

.DEFAULT_GOAL := help
SHELL := /usr/bin/env bash

CARGO_LEPTOS_VERSION := 0.3.7
WASM_BINDGEN_VERSION := 0.2.127

# Colours only when talking to a terminal.
BOLD := $(shell [ -t 1 ] && printf '\033[1m')
DIM  := $(shell [ -t 1 ] && printf '\033[2m')
OFF  := $(shell [ -t 1 ] && printf '\033[0m')

.PHONY: help
help: ## Show this help
	@printf '$(BOLD)Tinkhaven Typing$(OFF)\n\n'
	@grep -hE '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "} {printf "  $(BOLD)%-18s$(OFF) %s\n", $$1, $$2}'
	@printf '\n$(DIM)Run "make hooks" once after cloning, and "make check" before pushing.$(OFF)\n'

# ---------------------------------------------------------------------------
# Security. These come first because they matter most in a public repository.
# ---------------------------------------------------------------------------

.PHONY: hooks
hooks: ## Install the git hooks that block committing or pushing a secret
	@git config core.hooksPath .githooks
	@chmod +x .githooks/* scripts/*.sh
	@printf '$(BOLD)hooks installed$(OFF) (core.hooksPath = .githooks)\n'
	@printf '  pre-commit  scans staged changes\n'
	@printf '  pre-push    scans every commit being published\n'

.PHONY: secrets
secrets: ## Scan every tracked file for credentials
	@./scripts/check-secrets.sh --tree

.PHONY: secrets-staged
secrets-staged: ## Scan what is currently staged
	@./scripts/check-secrets.sh --staged

.PHONY: secrets-history
secrets-history: ## Scan every blob in every commit, ever (slow, thorough)
	@./scripts/check-secrets.sh --history
	@if command -v gitleaks >/dev/null 2>&1; then \
		printf '\n$(BOLD)== gitleaks ==$(OFF)\n'; \
		gitleaks git --redact --no-banner .; \
	else \
		printf '\n$(DIM)gitleaks not installed — "brew install gitleaks" for a second opinion$(OFF)\n'; \
	fi

.PHONY: sign-in
sign-in: ## Store the Google OAuth credentials in SSM (interactive)
	@./scripts/configure-sign-in.sh

.PHONY: audit
audit: ## Check dependencies for known vulnerabilities
	@if command -v cargo-audit >/dev/null 2>&1; then \
		cargo audit; \
	else \
		printf '$(DIM)cargo-audit not installed: cargo install cargo-audit$(OFF)\n'; \
	fi

# ---------------------------------------------------------------------------
# Code
# ---------------------------------------------------------------------------

.PHONY: fmt
fmt: ## Format the Rust and Terraform sources
	cargo fmt --all
	@command -v terraform >/dev/null 2>&1 && terraform -chdir=infra fmt || true

.PHONY: fmt-check
fmt-check: ## Verify formatting without changing anything
	cargo fmt --all -- --check
	@command -v terraform >/dev/null 2>&1 && terraform -chdir=infra fmt -check || true

.PHONY: lint
lint: ## Clippy, on both the server and the client target
	cargo clippy --workspace --all-targets --no-default-features --features typing-web/ssr -- -D warnings
	cargo clippy -p typing-web --no-default-features --features hydrate \
		--target wasm32-unknown-unknown -- -D warnings

.PHONY: test
test: ## Run the test suite
	cargo test -p typing-core
	cargo test -p typing-web --no-default-features --features ssr

.PHONY: build
build: ## Build the site (client + server) for development
	cargo leptos build

.PHONY: release
release: ## Build the site optimised for release
	cargo leptos build --release

.PHONY: serve
serve: ## Run the app locally on http://localhost:8080
	cargo leptos serve

.PHONY: smoke
smoke: ## Drive the practice socket end to end (needs a running server)
	python3 tests/ws_smoke.py

# ---------------------------------------------------------------------------
# The gates
# ---------------------------------------------------------------------------

.PHONY: check
check: secrets fmt-check lint test ## Everything to run before pushing
	@printf '\n$(BOLD)all checks passed$(OFF)\n'

.PHONY: ci
ci: secrets-history fmt-check lint test ## What the pipeline runs
	@printf '\n$(BOLD)ci checks passed$(OFF)\n'

# ---------------------------------------------------------------------------
# Container and deployment
# ---------------------------------------------------------------------------

.PHONY: docker
docker: ## Build the container image
	docker build --platform linux/arm64 -t tinkhaven-typing:local .

.PHONY: docker-run
docker-run: ## Run the container image on http://localhost:8080
	docker run --rm -p 8080:8080 tinkhaven-typing:local

.PHONY: tf-validate
tf-validate: ## Validate the Terraform configuration
	terraform -chdir=infra init -backend=false -input=false
	terraform -chdir=infra validate

.PHONY: deploy
deploy: check ## Build, push and roll the ECS service (runs make check first)
	./deploy.sh

.PHONY: tools
tools: ## Install the build toolchain at the pinned versions
	rustup target add wasm32-unknown-unknown
	cargo install --locked cargo-leptos --version $(CARGO_LEPTOS_VERSION)
	cargo install --locked wasm-bindgen-cli --version $(WASM_BINDGEN_VERSION)
	@printf '$(DIM)wasm-bindgen CLI and crate versions must match; bump both together.$(OFF)\n'

.PHONY: clean
clean: ## Remove build output
	cargo clean
	rm -rf target/site target/front
