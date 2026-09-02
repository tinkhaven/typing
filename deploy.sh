#!/usr/bin/env bash
#
# Build the image, push it to ECR, and roll the ECS service.
#
# Provision the infrastructure first:
#
#   export AWS_PROFILE=<your-admin-profile>
#   cp infra/terraform.tfvars.example infra/terraform.tfvars   # then edit it
#   terraform -chdir=infra init
#   terraform -chdir=infra apply
#
# Then every deploy is just: ./deploy.sh
set -euo pipefail

# Required rather than defaulted. Naming a specific profile in a public
# repository would publish internal account naming for no benefit, and a wrong
# default is worse than no default when the command it feeds deploys things.
: "${AWS_PROFILE:?set AWS_PROFILE to the profile that can deploy, e.g. export AWS_PROFILE=my-admin-profile}"
export AWS_PROFILE

cd "$(dirname "$0")"

say() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
die() { printf '\033[31merror: %s\033[0m\n' "$*" >&2; exit 1; }

command -v docker >/dev/null || die "docker is not installed"
command -v terraform >/dev/null || die "terraform is not installed"
command -v aws >/dev/null || die "the AWS CLI is not installed"
docker info >/dev/null 2>&1 || die "docker is installed but not running"

say "Reading infrastructure outputs"
tf() { terraform -chdir=infra output -raw "$1" 2>/dev/null || true; }
REGISTRY="$(tf ecr_repository_url)"
CLUSTER="$(tf ecs_cluster)"
SERVICE="$(tf ecs_service)"
SITE_URL="$(tf site_url)"
REGION="$(terraform -chdir=infra output -raw aws_region 2>/dev/null || echo eu-west-1)"

[ -n "$REGISTRY" ] || die "no terraform outputs; run 'terraform -chdir=infra apply' first"

# Match the architecture the task definition declares, so the image actually runs.
ARCH="$(grep -E '^\s*(#\s*)?cpu_architecture' infra/terraform.tfvars 2>/dev/null \
        | grep -v '^\s*#' | sed 's/.*=\s*"\(.*\)".*/\1/' || true)"
ARCH="${ARCH:-ARM64}"
case "$ARCH" in
  ARM64)  PLATFORM=linux/arm64 ;;
  X86_64) PLATFORM=linux/amd64 ;;
  *) die "unknown cpu_architecture: $ARCH" ;;
esac

# Tag with the commit as well as :latest. The task definition points at :latest,
# but the SHA tag makes it possible to tell later which build is running.
VERSION="$(git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S)"
if ! git diff --quiet 2>/dev/null || ! git diff --cached --quiet 2>/dev/null; then
  VERSION="${VERSION}-dirty"
fi

# Braces are load-bearing here. This script runs under bash, but in zsh
# "$REGISTRY:latest" parses ":l" as the lowercase modifier and silently builds
# a tag called "…/typingatest:latest" instead — which then never gets pulled.
say "Building $PLATFORM image ($VERSION)"
docker build --platform "$PLATFORM" -t "${REGISTRY}:latest" -t "${REGISTRY}:${VERSION}" .

say "Pushing to ECR"
aws ecr get-login-password --region "$REGION" \
  | docker login --username AWS --password-stdin "${REGISTRY%%/*}"
docker push "${REGISTRY}:latest"
docker push "${REGISTRY}:${VERSION}"

say "Rolling the service"
aws ecs update-service \
  --cluster "$CLUSTER" \
  --service "$SERVICE" \
  --force-new-deployment \
  --region "$REGION" \
  --no-cli-pager \
  --query 'service.{service:serviceName,desired:desiredCount}' \
  --output table

say "Waiting for the new task to be healthy (a few minutes)"
aws ecs wait services-stable \
  --cluster "$CLUSTER" \
  --service "$SERVICE" \
  --region "$REGION"

say "Deployed: $SITE_URL"
curl -sS -o /dev/null -w 'health check: HTTP %{http_code}\n' "$SITE_URL/health" || true
