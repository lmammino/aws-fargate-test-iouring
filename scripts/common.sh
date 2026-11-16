# Shared helpers. Sourced by the other scripts, not run directly.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STACK_NAME="luciano-experiment-fargate-iouring"
# shellcheck disable=SC2034
TEMPLATE="$ROOT_DIR/infra/cloudformation/template.yaml"
# shellcheck disable=SC2034 # used by the scripts that source this file
CONTAINER_NAME="probe"
# shellcheck disable=SC2034
RESULT_PREFIX="IOURING_PROBE_RESULT"
# shellcheck disable=SC2034
RESULTS_DIR="$ROOT_DIR/results"

log() { printf '[%s] %s\n' "$(basename "$0")" "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

require_tools() {
  for tool in "$@"; do
    command -v "$tool" >/dev/null || die "missing required tool: $tool"
  done
}

# Refuses to continue unless the caller is in EXPECTED_ACCOUNT_ID and a region is set.
verify_identity() {
  [[ -n "${EXPECTED_ACCOUNT_ID:-}" ]] || die "set EXPECTED_ACCOUNT_ID to the target AWS account id"

  AWS_REGION="${AWS_REGION:-$(aws configure get region || true)}"
  [[ -n "$AWS_REGION" ]] || die "set AWS_REGION (or a region in the AWS profile)"
  export AWS_REGION AWS_DEFAULT_REGION="$AWS_REGION"

  local identity account arn
  identity="$(aws sts get-caller-identity --output json)" || die "cannot call sts get-caller-identity"
  account="$(jq -r .Account <<<"$identity")"
  arn="$(jq -r .Arn <<<"$identity")"

  [[ "$account" == "$EXPECTED_ACCOUNT_ID" ]] \
    || die "credentials belong to account $account, expected $EXPECTED_ACCOUNT_ID"

  log "verified identity: $arn (account $account, region $AWS_REGION)"
}

stack_output() {
  aws cloudformation describe-stacks --stack-name "$STACK_NAME" \
    --query "Stacks[0].Outputs[?OutputKey=='$1'].OutputValue" --output text
}

# Content hash of the probe sources, so unchanged code reuses the same image tag.
image_tag() {
  (cd "$ROOT_DIR/app" && cat Cargo.toml Cargo.lock build.rs Dockerfile src/*.rs) \
    | shasum -a 256 | cut -c1-12
}
