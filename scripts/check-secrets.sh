#!/usr/bin/env bash
#
# Refuse to let a secret into a public repository.
#
# The same checks run in three places, so what a hook blocks is exactly what CI
# blocks and exactly what you can reproduce by hand:
#
#   scripts/check-secrets.sh --staged        what you are about to commit
#   scripts/check-secrets.sh --tree          the whole working tree
#   scripts/check-secrets.sh --range A..B    every blob in a commit range
#   scripts/check-secrets.sh --history       every blob in every commit, ever
#
# --range is what the pre-push hook uses: it covers commits that are about to
# become public, including anything they touched that is no longer in the tree,
# without paying for a full-history walk on every push.
#
# Design note: the rules are deliberately *precise* rather than clever. Entropy
# heuristics find more in theory, but they also fire on `Cargo.lock` checksums
# and on Dutch prose in the practice corpora — and a hook that cries wolf is a
# hook someone disables with --no-verify, which is worse than no hook. Every
# rule here targets a recognisable credential shape.
#
# False positives go in .secretsallow (one extended regex per line) with a
# comment saying why. Never add a rule-wide exemption to silence one file.
set -uo pipefail

MODE="${1:-}"
RANGE="${2:-}"
[ -n "$MODE" ] || MODE="--tree"

RED=$'\033[31m'; YELLOW=$'\033[33m'; GREEN=$'\033[32m'; BOLD=$'\033[1m'; OFF=$'\033[0m'
[ -t 1 ] || { RED=''; YELLOW=''; GREEN=''; BOLD=''; OFF=''; }

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

ALLOWLIST="$REPO_ROOT/.secretsallow"

# Findings are recorded in a file rather than a counter. report() is reached from
# inside pipelines, and a pipeline stage is a subshell, so `findings=$((...))`
# there increments a copy that is discarded when the stage exits — which made an
# earlier version of this script print every finding and then exit 0.
FINDINGS_FILE="$(mktemp "${TMPDIR:-/tmp}/secretscan.XXXXXX")"
trap 'rm -f "$FINDINGS_FILE"' EXIT INT TERM

# --------------------------------------------------------------------------
# Rules. Each is "name<TAB>extended regex".
#
# Anchored on provider prefixes and unmistakable headers, so a hit is a hit.
# --------------------------------------------------------------------------
read -r -d '' RULES <<'RULESET' || true
AWS access key id	(A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}
AWS secret access key	aws_secret_access_key[[:space:]]*=[[:space:]]*[^[:space:]"'$]{20,}
AWS session token	aws_session_token[[:space:]]*=[[:space:]]*[^[:space:]"'$]{20,}
AWS account id in an ARN	arn:aws[a-z-]*:[a-z0-9-]*:[a-z0-9-]*:[0-9]{12}:
Private key block	-----BEGIN [A-Z ]*PRIVATE KEY-----
OpenSSH private key	-----BEGIN OPENSSH PRIVATE KEY-----
PGP private key	-----BEGIN PGP PRIVATE KEY BLOCK-----
GitHub token	gh[pousr]_[A-Za-z0-9]{36}
GitHub fine-grained PAT	github_pat_[A-Za-z0-9_]{22,}
Anthropic API key	sk-ant-[A-Za-z0-9_-]{16,}
OpenAI API key	sk-(proj-)?[A-Za-z0-9]{32,}
Slack token	xox[baprs]-[A-Za-z0-9-]{10,}
Slack webhook	hooks\.slack\.com/services/T[A-Za-z0-9_/]{20,}
Google API key	AIza[0-9A-Za-z_-]{35}
Stripe secret key	(sk|rk)_(live|test)_[0-9a-zA-Z]{20,}
Twilio key	SK[0-9a-fA-F]{32}
SendGrid key	SG\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}
NPM token	npm_[A-Za-z0-9]{36}
PyPI token	pypi-AgEIcHlwaS5vcmc[A-Za-z0-9_-]{20,}
Hashicorp Vault token	hv[sb]\.[A-Za-z0-9_-]{20,}
JSON web token	eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}
Generic assigned secret	(password|passwd|secret|api[_-]?key|apikey|access[_-]?token|auth[_-]?token|client[_-]?secret|private[_-]?key)[[:space:]]*[:=][[:space:]]*("|')[^"']{8,}("|')
Basic auth in a URL	[a-z][a-z0-9+.-]*://[^/[:space:]:@]+:[^/[:space:]@]+@
Local filesystem path	/(Users|home)/[a-z][a-z0-9._-]+/
RULESET

# --------------------------------------------------------------------------
# Filenames that must never be tracked, whatever they contain.
# --------------------------------------------------------------------------
FORBIDDEN_PATHS='(^|/)\.env($|\.)|(^|/)\.envrc$|\.pem$|\.p12$|\.pfx$|\.jks$|\.keystore$|(^|/)id_(rsa|dsa|ecdsa|ed25519)$|(^|/)\.netrc$|(^|/)\.npmrc$|(^|/)\.pypirc$|(^|/)credentials$|(^|/)\.aws/|\.tfstate($|\.)|\.tfvars$|(^|/)tfplan($|\.)|\.tfplan($|\.)|(^|/)kubeconfig$|(^|/)\.ssh/|\.kdbx$|(^|/)secrets?\.(ya?ml|json|toml)$'

# The only path exempt from content rules: this script, whose rule list
# necessarily contains the very key headers it looks for. Filename rules still
# apply to it, and gitleaks plus GitHub's push protection cover it independently
# — neither knows about this list.
#
# Nothing else is exempt. `Cargo.lock`, the GPL text and 38 languages of practice
# prose were all exempted at first on the assumption they would trip the rules;
# running the scan without those exemptions found nothing, so they were removed.
# Every exemption is a hole, and this one was not paying for itself.
CONTENT_EXEMPT='^(scripts/check-secrets\.sh:)'

allowed() {
  # $1 = "path:line:text". True if the allowlist forgives it.
  [ -f "$ALLOWLIST" ] || return 1
  local line
  while IFS= read -r line; do
    case "$line" in ''|'#'*) continue ;; esac
    printf '%s' "$1" | grep -qE -- "$line" && return 0
  done < "$ALLOWLIST"
  return 1
}

report() {
  # $1 = rule name, $2 = "path:line:text"
  local text
  text="$(printf '%s' "$2" | cut -c1-160)"
  printf '%s%s FAIL%s %s\n' "$RED" "$BOLD" "$OFF" "$1"
  printf '        %s\n' "$text"
  printf '%s\n' "$1" >> "$FINDINGS_FILE"
}

scan_content() {
  # Reads "path:line:text" on stdin and applies every content rule.
  local input rule name regex hit
  input="$(cat)"
  [ -n "$input" ] || return 0
  while IFS=$'\t' read -r name regex; do
    [ -n "${name:-}" ] || continue
    while IFS= read -r hit; do
      [ -n "$hit" ] || continue
      allowed "$hit" && continue
      report "$name" "$hit"
    done < <(printf '%s\n' "$input" | grep -E -- "$regex" 2>/dev/null | grep -vE "$CONTENT_EXEMPT" || true)
  done <<< "$RULES"
}

check_filenames() {
  local paths hit
  paths="$(cat)"
  [ -n "$paths" ] || return 0
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    allowed "$hit" && continue
    report "Forbidden filename" "$hit"
  done < <(printf '%s\n' "$paths" | grep -E -- "$FORBIDDEN_PATHS" || true)
}

scanned=0
printf '%s== secret scan (%s) ==%s\n' "$BOLD" "${MODE#--}" "$OFF"

case "$MODE" in
  --staged)
    STAGED="$(git diff --cached --name-only --diff-filter=ACMR)"
    if [ -z "$STAGED" ]; then
      printf '  nothing staged\n'
      exit 0
    fi
    scanned="$(printf '%s\n' "$STAGED" | grep -c . || true)"
    printf '%s\n' "$STAGED" | check_filenames
    # Scan added lines only, so an untouched historical line cannot block work
    # that has nothing to do with it. --history covers the rest.
    git diff --cached --unified=0 -- $(printf '%s\n' "$STAGED" | tr '\n' ' ') 2>/dev/null \
      | awk '
          /^\+\+\+ b\// { path = substr($0, 7); next }
          /^@@/ { split($0, a, "+"); split(a[2], b, /[, ]/); line = b[1] + 0; next }
          /^\+/ { print path ":" line ":" substr($0, 2); line++ }
        ' \
      | scan_content
    ;;
  --tree)
    # Tracked *and* untracked-but-not-ignored. Scanning only tracked files would
    # report "clean" while a brand new file full of keys sat beside it waiting to
    # be added — the exact moment the check is most useful.
    FILES="$(git ls-files --cached --others --exclude-standard)"
    scanned="$(printf '%s\n' "$FILES" | grep -c . || true)"
    printf '%s\n' "$FILES" | check_filenames
    while IFS= read -r file; do
      [ -f "$file" ] || continue
      grep -nI -e '' -- "$file" 2>/dev/null | sed "s|^|$file:|" | scan_content
    done <<< "$FILES"
    ;;
  --range|--history)
    if [ "$MODE" = "--range" ]; then
      [ -n "$RANGE" ] || { printf 'usage: %s --range A..B\n' "$0" >&2; exit 2; }
      REVS="$RANGE"
      # An unresolvable range must be an error, never a pass. Suppressing
      # rev-list's failure would scan zero blobs and cheerfully report "clean".
      if ! git rev-list --objects $REVS >/dev/null 2>&1; then
        printf '%s%s ERROR%s cannot resolve range %s\n' "$RED" "$BOLD" "$OFF" "$RANGE" >&2
        exit 2
      fi
    else
      REVS="--all"
    fi
    # shellcheck disable=SC2086
    git rev-list --objects $REVS | awk 'NF>1 {print $2}' | sort -u | check_filenames
    # Every blob in range, including ones no longer in the tree.
    while read -r object path; do
      [ -n "$path" ] || continue
      git cat-file -p "$object" 2>/dev/null \
        | grep -nI -e '' 2>/dev/null \
        | sed "s|^|$path:|" \
        | scan_content
      scanned=$((scanned + 1))
    done < <(git rev-list --objects $REVS | awk 'NF>1')
    ;;
  *)
    printf 'usage: %s [--staged|--tree|--range A..B|--history]\n' "$0" >&2
    exit 2
    ;;
esac

findings="$(wc -l < "$FINDINGS_FILE" | tr -d ' ')"
if [ "${findings:-0}" -gt 0 ]; then
  cat >&2 <<EOF

${RED}${BOLD}$findings finding(s). Nothing was committed or pushed.${OFF}

If it is a real secret:
  1. Do not just delete the line — rotate the credential. Assume it is burned.
  2. Remove it from the working tree, then re-run this check.
  3. If it is already in a commit, the history needs rewriting (git filter-repo)
     and the credential rotating regardless.

If it is a false positive, add a narrow regex to .secretsallow with a comment
explaining why it is safe. Do not widen a rule and do not use --no-verify.
EOF
  exit 1
fi

if [ "${scanned:-0}" -eq 0 ]; then
  printf '%s  nothing to scan%s\n' "$YELLOW" "$OFF"
else
  printf '%s  clean%s (%s file(s)/blob(s) scanned)\n' "$GREEN" "$OFF" "$scanned"
fi
