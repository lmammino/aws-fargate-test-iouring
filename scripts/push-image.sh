#!/usr/bin/env bash
# Builds the linux/amd64 probe image and pushes it to the stack's ECR repo
# as `latest` (used by the task definition) and as a content-hash tag.
# Usage: EXPECTED_ACCOUNT_ID=... AWS_REGION=... [AWS_PROFILE=...] scripts/push-image.sh
source "$(dirname "$0")/common.sh"
require_tools aws jq docker shasum
verify_identity

repo_uri="$(stack_output RepositoryUri)"
[[ -n "$repo_uri" && "$repo_uri" != "None" ]] || die "stack $STACK_NAME not deployed or has no RepositoryUri output"
registry="${repo_uri%%/*}"
tag="$(image_tag)"

log "building linux/amd64 image ($tag)"
docker build --platform linux/amd64 -t "$repo_uri:$tag" -t "$repo_uri:latest" "$ROOT_DIR/app"

aws ecr get-login-password | docker login --username AWS --password-stdin "$registry"
docker push "$repo_uri:$tag"
docker push "$repo_uri:latest"
log "pushed $repo_uri:latest ($tag)"
