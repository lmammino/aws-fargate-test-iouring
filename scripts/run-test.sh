#!/usr/bin/env bash
# Runs one standalone Fargate task, waits for STOPPED, then collects results.
# Usage: EXPECTED_ACCOUNT_ID=... AWS_REGION=... [PLATFORM_VERSION=LATEST] scripts/run-test.sh
source "$(dirname "$0")/common.sh"
require_tools aws jq
verify_identity

cluster="$(stack_output ClusterName)"
task_def="$(stack_output TaskDefinitionArn)"
subnets="$(stack_output SubnetIds)"
security_group="$(stack_output SecurityGroupId)"
[[ -n "$cluster" && "$cluster" != "None" ]] || die "stack $STACK_NAME not deployed; run scripts/deploy.sh"

# Public IP only for outbound ECR and Logs traffic. The security group has no inbound rules.
network="awsvpcConfiguration={subnets=[$subnets],securityGroups=[$security_group],assignPublicIp=ENABLED}"

log "running task $task_def on $cluster (platform ${PLATFORM_VERSION:-LATEST})"
run_output="$(aws ecs run-task \
  --cluster "$cluster" \
  --task-definition "$task_def" \
  --launch-type FARGATE \
  --platform-version "${PLATFORM_VERSION:-LATEST}" \
  --network-configuration "$network" \
  --propagate-tags TASK_DEFINITION \
  --count 1 --output json)"

failures="$(jq '.failures | length' <<<"$run_output")"
[[ "$failures" == "0" ]] || die "RunTask failures: $(jq -c .failures <<<"$run_output")"

task_arn="$(jq -r '.tasks[0].taskArn' <<<"$run_output")"
log "started $task_arn, waiting for STOPPED"
aws ecs wait tasks-stopped --cluster "$cluster" --tasks "$task_arn"

exec "$(dirname "$0")/collect-results.sh" "$task_arn"
