#!/usr/bin/env bash
#
# Stores the Google sign-in credentials in SSM Parameter Store.
#
#   export AWS_PROFILE=<your-admin-profile>
#   ./scripts/configure-sign-in.sh
#
# Run this after creating the OAuth client in Google Cloud Console and before
# setting `enable_sign_in = true` in infra/terraform.tfvars.
#
# The client secret is read from the terminal without echo and handed to the AWS
# CLI on stdin, never as a command-line argument — arguments are visible in `ps`
# to anything running as you, and end up in shell history if typed directly.
# Nothing is written to disk.
set -euo pipefail

: "${AWS_PROFILE:?set AWS_PROFILE to the profile that can write parameters, e.g. export AWS_PROFILE=my-admin-profile}"
export AWS_PROFILE

cd "$(dirname "$0")/.."

BOLD=$'\033[1m'; DIM=$'\033[2m'; RED=$'\033[31m'; GREEN=$'\033[32m'; OFF=$'\033[0m'
[ -t 1 ] || { BOLD=''; DIM=''; RED=''; GREEN=''; OFF=''; }

say()  { printf '\n%s==> %s%s\n' "$BOLD" "$*" "$OFF"; }
die()  { printf '%serror: %s%s\n' "$RED" "$*" "$OFF" >&2; exit 1; }

command -v aws >/dev/null || die "the AWS CLI is not installed"

# awk on the quoted value rather than sed with \s: BSD sed, which is what macOS
# ships, does not understand \s and silently returns the whole line.
tfvar() {
  awk -F'"' -v key="$1" '
    $0 ~ "^[[:space:]]*" key "[[:space:]]*=" { print $2; exit }
  ' infra/terraform.tfvars 2>/dev/null
}

REGION="$(terraform -chdir=infra output -raw aws_region 2>/dev/null || true)"
REGION="${REGION:-$(tfvar aws_region)}"
REGION="${REGION:-eu-west-1}"

PREFIX="$(tfvar ssm_prefix)"
PREFIX="${PREFIX:-/typing}"

DOMAIN="$(tfvar domain_name)"
[ -n "$DOMAIN" ] || die "could not read domain_name from infra/terraform.tfvars"
case "$DOMAIN" in
  *.*) ;;
  *) die "domain_name in infra/terraform.tfvars does not look like a hostname: $DOMAIN" ;;
esac

say "Target"
printf '  profile:  %s\n  region:   %s\n  prefix:   %s\n  redirect: %s\n' \
  "$AWS_PROFILE" "$REGION" "$PREFIX" "https://${DOMAIN}/auth/google/callback"

printf '\n%sRegister exactly this as the authorised redirect URI in Google Cloud Console:%s\n' "$BOLD" "$OFF"
printf '  https://%s/auth/google/callback\n' "$DOMAIN"
printf '%s  (and http://localhost:8080/auth/google/callback if you want local sign-in)%s\n' "$DIM" "$OFF"

say "Credentials"
printf 'Client ID (not secret, will be shown): '
read -r CLIENT_ID
[ -n "$CLIENT_ID" ] || die "the client id is required"
case "$CLIENT_ID" in
  *.apps.googleusercontent.com) ;;
  *) printf '%s  note: that does not look like a Google client id (expected …apps.googleusercontent.com)%s\n' "$DIM" "$OFF" ;;
esac

printf 'Client secret (hidden): '
read -rs CLIENT_SECRET
printf '\n'
[ -n "$CLIENT_SECRET" ] || die "the client secret is required"

# Generated here rather than asked for: it never needs to be seen or kept, and
# a rolled session secret orphans every stored profile, so it should be created
# once by something that will not be tempted to write it down.
say "Session secret"
if aws ssm get-parameter --name "${PREFIX}/session_secret" --region "$REGION" >/dev/null 2>&1; then
  printf '  already exists — leaving it alone.\n'
  printf '%s  Replacing it would change every derived user id and orphan every profile.%s\n' "$DIM" "$OFF"
  WRITE_SESSION_SECRET=false
else
  printf '  generating 48 random bytes\n'
  WRITE_SESSION_SECRET=true
fi

# --cli-input-json on stdin keeps every value out of argv.
put_parameter() {
  local name="$1" type="$2" value="$3"
  python3 - "$name" "$type" <<'PY' | aws ssm put-parameter --region "$REGION" --cli-input-json file:///dev/stdin >/dev/null
import json, sys, os
print(json.dumps({
    "Name": sys.argv[1],
    "Type": sys.argv[2],
    "Value": os.environ["PARAM_VALUE"],
    "Overwrite": True,
}))
PY
}

say "Writing parameters"
PARAM_VALUE="$CLIENT_ID"     put_parameter "${PREFIX}/google_client_id"     String       && printf '  %s✓%s %s/google_client_id\n' "$GREEN" "$OFF" "$PREFIX"
PARAM_VALUE="$CLIENT_SECRET" put_parameter "${PREFIX}/google_client_secret" SecureString && printf '  %s✓%s %s/google_client_secret\n' "$GREEN" "$OFF" "$PREFIX"
if [ "$WRITE_SESSION_SECRET" = true ]; then
  PARAM_VALUE="$(openssl rand -base64 48)" put_parameter "${PREFIX}/session_secret" SecureString \
    && printf '  %s✓%s %s/session_secret\n' "$GREEN" "$OFF" "$PREFIX"
fi
unset CLIENT_SECRET PARAM_VALUE

say "Verifying they are readable"
for name in google_client_id google_client_secret session_secret; do
  if aws ssm get-parameter --name "${PREFIX}/${name}" --region "$REGION" \
       --query 'Parameter.Type' --output text >/dev/null 2>&1; then
    printf '  %s✓%s %s/%s\n' "$GREEN" "$OFF" "$PREFIX" "$name"
  else
    die "${PREFIX}/${name} is not readable"
  fi
done

cat <<EOF

${BOLD}Next${OFF}
  1. Add this to infra/terraform.tfvars:

       enable_sign_in = true

  2. Apply and deploy:

       terraform -chdir=infra apply
       ./deploy.sh

  3. Check it took:

       curl -s https://${DOMAIN}/api/me
       # expect {"available":true,"signed_in":false}

${DIM}The client secret was not written to disk or to your shell history, and does
not appear in the Terraform state — Terraform references these by ARN only.${OFF}
EOF
