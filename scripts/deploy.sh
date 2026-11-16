#!/usr/bin/env bash
# Deploys infra/cloudformation/template.yaml, then pushes the probe image.
# You can instead deploy the template yourself (console or CLI) with the
# stack name luciano-experiment-fargate-iouring and run scripts/push-image.sh.
# Usage: EXPECTED_ACCOUNT_ID=... AWS_REGION=... [VPC_ID=... SUBNET_IDS=a,b] scripts/deploy.sh
source "$(dirname "$0")/common.sh"
require_tools aws jq
verify_identity

params=()
if [[ -n "${VPC_ID:-}" ]]; then
  [[ -n "${SUBNET_IDS:-}" ]] || die "SUBNET_IDS (public subnets) is required with VPC_ID"
  params=(--parameter-overrides "ExistingVpcId=$VPC_ID" "ExistingPublicSubnetIds=$SUBNET_IDS")
fi

aws cloudformation deploy \
  --stack-name "$STACK_NAME" \
  --template-file "$TEMPLATE" \
  --capabilities CAPABILITY_IAM \
  --tags "Project=$STACK_NAME" \
  --no-fail-on-empty-changeset \
  ${params[@]+"${params[@]}"}

exec "$(dirname "$0")/push-image.sh"
