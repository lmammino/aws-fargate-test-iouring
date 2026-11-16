#!/usr/bin/env bash
# Runs the probe once on the ECS on EC2 stack, on the container instance with
# the requested seccomp setup, waits for STOPPED, then collects results.
# Usage: EXPECTED_ACCOUNT_ID=... AWS_REGION=... scripts/run-test-ec2.sh <docker-default|custom-iouring>
export STACK_NAME="${STACK_NAME:-luciano-experiment-ecs-ec2-iouring}"
source "$(dirname "$0")/common.sh"
require_tools aws jq
verify_identity

ATTRIBUTE="iouring-experiment.seccomp"
REGISTRATION_ATTEMPTS=30
REGISTRATION_INTERVAL_SECONDS=10

profile="${1:-}"
case "$profile" in
  docker-default|custom-iouring) ;;
  *) die "usage: $0 <docker-default|custom-iouring>" ;;
esac

cluster="$(stack_output ClusterName)"
task_def="$(stack_output TaskDefinitionArn)"
[[ -n "$cluster" && "$cluster" != "None" ]] || die "stack $STACK_NAME not deployed"
constraint="attribute:$ATTRIBUTE == $profile"

# Instances register with ECS a few minutes after the stack completes.
for attempt in $(seq "$REGISTRATION_ATTEMPTS"); do
  instances="$(aws ecs list-container-instances --cluster "$cluster" --filter "$constraint" \
    --query 'length(containerInstanceArns)' --output text)"
  [[ "$instances" -gt 0 ]] && break
  [[ "$attempt" -eq "$REGISTRATION_ATTEMPTS" ]] \
    && die "no container instance with $constraint. Check the instance console output (aws ec2 get-console-output)"
  log "waiting for a container instance with $constraint (attempt $attempt)"
  sleep "$REGISTRATION_INTERVAL_SECONDS"
done

log "running task $task_def on $cluster ($constraint)"
run_output="$(aws ecs run-task \
  --cluster "$cluster" \
  --task-definition "$task_def" \
  --launch-type EC2 \
  --placement-constraints "type=memberOf,expression=$constraint" \
  --propagate-tags TASK_DEFINITION \
  --count 1 --output json)"

failures="$(jq '.failures | length' <<<"$run_output")"
[[ "$failures" == "0" ]] || die "RunTask failures: $(jq -c .failures <<<"$run_output")"

task_arn="$(jq -r '.tasks[0].taskArn' <<<"$run_output")"
log "started $task_arn, waiting for STOPPED"
aws ecs wait tasks-stopped --cluster "$cluster" --tasks "$task_arn"

export RESULT_LABEL="ec2-$profile"
exec "$(dirname "$0")/collect-results.sh" "$task_arn"
