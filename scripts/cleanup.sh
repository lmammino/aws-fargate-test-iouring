#!/usr/bin/env bash
# Stops running probe tasks and deletes the stack: ECR repo and images, log
# group, cluster, task definition, role, security group and (if created) VPC.
# Local results/ are kept.
# Usage: EXPECTED_ACCOUNT_ID=... AWS_REGION=... scripts/cleanup.sh --yes
source "$(dirname "$0")/common.sh"
require_tools aws jq
verify_identity

[[ "${1:-}" == "--yes" ]] || die "this deletes all experiment resources, including logs and images. Re-run with --yes"

if ! aws cloudformation describe-stacks --stack-name "$STACK_NAME" >/dev/null 2>&1; then
  log "stack $STACK_NAME not found, nothing to delete"
  exit 0
fi

cluster="$(stack_output ClusterName)"
for task in $(aws ecs list-tasks --cluster "$cluster" --desired-status RUNNING --query 'taskArns[]' --output text); do
  log "stopping $task"
  aws ecs stop-task --cluster "$cluster" --task "$task" --reason "experiment cleanup" >/dev/null
done

log "deleting stack $STACK_NAME"
aws cloudformation delete-stack --stack-name "$STACK_NAME"
aws cloudformation wait stack-delete-complete --stack-name "$STACK_NAME"
log "stack deleted"

# Deleted task definition revisions stay INACTIVE; remove them fully.
for arn in $(aws ecs list-task-definitions --family-prefix "$STACK_NAME" --status INACTIVE --query 'taskDefinitionArns[]' --output text); do
  aws ecs delete-task-definitions --task-definitions "$arn" >/dev/null && log "deleted $arn"
done

remaining="$(aws resourcegroupstaggingapi get-resources --tag-filters "Key=Project,Values=$STACK_NAME" \
  --query 'ResourceTagMappingList[].ResourceARN' --output text)"
log "resources still tagged Project=$STACK_NAME: ${remaining:-none}"
