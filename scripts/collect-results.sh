#!/usr/bin/env bash
# Saves task metadata, CloudWatch logs and the probe JSON for one stopped task.
# Usage: EXPECTED_ACCOUNT_ID=... AWS_REGION=... scripts/collect-results.sh <task-arn-or-id>
source "$(dirname "$0")/common.sh"
require_tools aws jq
verify_identity

[[ $# -eq 1 ]] || die "usage: $0 <task-arn-or-id>"
cluster="$(stack_output ClusterName)"
log_group="$(stack_output LogGroupName)"

task_json="$(aws ecs describe-tasks --cluster "$cluster" --tasks "$1" --output json)"
task="$(jq '.tasks[0]' <<<"$task_json")"
[[ "$task" != "null" ]] || die "task $1 not found (ECS keeps stopped tasks for about 1 hour)"

task_id="$(jq -r '.taskArn | split("/") | last' <<<"$task")"
out_dir="$RESULTS_DIR/$(date -u +%Y%m%dT%H%M%SZ)-$task_id"
mkdir -p "$out_dir"
jq . <<<"$task" > "$out_dir/task.json"

stream="probe/$CONTAINER_NAME/$task_id"
log "reading log stream $log_group/$stream"

# Log delivery can lag a few seconds behind task stop.
for attempt in 1 2 3 4 5 6; do
  aws logs get-log-events --log-group-name "$log_group" --log-stream-name "$stream" \
    --start-from-head --output json > "$out_dir/log-events.json" 2>/dev/null || true
  jq -r '.events[]?.message' "$out_dir/log-events.json" > "$out_dir/probe.log" 2>/dev/null || true
  grep -q "^$RESULT_PREFIX " "$out_dir/probe.log" && break
  log "result line not in logs yet (attempt $attempt), retrying"
  sleep 5
done

if grep -q "^$RESULT_PREFIX " "$out_dir/probe.log"; then
  grep "^$RESULT_PREFIX " "$out_dir/probe.log" | tail -1 | sed "s/^$RESULT_PREFIX //" | jq . > "$out_dir/result.json"
else
  log "WARNING: no $RESULT_PREFIX line found; inspect $out_dir/probe.log and task.json"
fi

jq '{
  taskArn, lastStatus, stopCode, stoppedReason, platformVersion, platformFamily,
  cpu, memory, availabilityZone, createdAt, startedAt, stoppedAt,
  container: (.containers[0] | {name, image, imageDigest, exitCode, reason, lastStatus})
}' "$out_dir/task.json" > "$out_dir/summary.json"

if [[ -f "$out_dir/result.json" ]]; then
  jq --slurpfile r "$out_dir/result.json" '. + {probe_summary: $r[0].summary, kernel: $r[0].environment.kernel.release}' \
    "$out_dir/summary.json" > "$out_dir/summary.tmp" && mv "$out_dir/summary.tmp" "$out_dir/summary.json"
fi

cat "$out_dir/summary.json"
log "results saved in $out_dir"
