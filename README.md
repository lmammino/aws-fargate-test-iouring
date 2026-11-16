# io_uring on ECS Fargate: proof of concept

Runs a small Rust probe as a standalone ECS Fargate task to find out, with
evidence, whether Linux `io_uring` works there. An optional ECS on EC2
comparison checks whether the container seccomp profile is the cause.

- Research and expectations: [docs/RESEARCH.md](docs/RESEARCH.md)
- Experiment results: [docs/RESULTS.md](docs/RESULTS.md)

## Spoilers

As of 2026-11-16, io_uring is **not available** on ECS Fargate (platform
1.4.0, kernel 6.1.189, eu-west-1). The kernel supports it, but
`io_uring_setup`, `io_uring_enter` and `io_uring_register` all fail with
`EPERM` before reaching the kernel, consistent with the container seccomp
filter. Fargate offers no setting to change this.

On **ECS on EC2** (AL2023, kernel 6.1.190) the result is the same with
Docker's default seccomp profile. Adding just the three io_uring syscalls to
that profile, as the Docker daemon default, makes every io_uring test pass,
with an unprivileged, non-root container. So io_uring on ECS is possible, but
only on EC2 instances whose Docker seccomp profile you change.

Details in [docs/RESULTS.md](docs/RESULTS.md).

## Layout

| Path | Contents |
| --- | --- |
| `app/` | Rust probe (`io-uring` crate + raw syscalls), multi-stage Dockerfile |
| `infra/cloudformation/` | CloudFormation templates: `template.yaml` (Fargate, primary), `ecs-ec2.yaml` (ECS on EC2 comparison) |
| `infra/` | Equivalent AWS CDK v2 (TypeScript) stack |
| `scripts/` | deploy, run, collect, cleanup |
| `results/` | Raw output of each Fargate run (created by `collect-results.sh`) |
| `docs/` | Research notes and final report |

## What the probe tests

| Test | Purpose |
| --- | --- |
| environment | `uname`, arch, capabilities, seccomp mode, `io_uring_disabled`/`io_uring_group`, `RLIMIT_MEMLOCK`, `/proc/kallsyms` io_uring symbol, ECS task metadata |
| syscall_reachability | Calls setup/enter/register with invalid args to tell seccomp, sysctl policy and missing kernel support apart (see RESEARCH.md) |
| direct_syscall | `io_uring_setup(8, {0})` via `syscall(2)` |
| ring_initialization | `IoUring::new(8)` and ring feature flags |
| nop, file_write, file_read | Core operations, with data verification |
| opcode_probe, registered_buffers, registered_files, sqpoll | Optional features. Never affect the classification |
| conventional_io | `std::fs` round trip control test |

Human-readable lines go to stderr. The JSON report is printed to stdout as one
line prefixed with `IOURING_PROBE_RESULT `, so CloudWatch keeps it as one event.
Pass `--pretty` for indented JSON without the prefix.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | `supported`: ring setup, NOP, write and read all passed |
| 10 | `partial`: io_uring reachable, at least one core test failed |
| 11 | `unavailable`: ring creation failed (valid experimental result) |
| 12 | `inconclusive`: conventional I/O control test failed |
| 2 | probe error (e.g. no writable temp dir) |
| 3 | watchdog timeout (60 s) |
| 101 | Rust panic (application bug) |

Codes 0, 10 and 11 are experimental results. The others mean the run did not
produce a trustworthy answer.

## Local usage

Requires Docker. On Apple Silicon, build a native arm64 image: x86_64 under
Rosetta returns `ENOSYS` for io_uring syscalls regardless of seccomp.

```sh
docker build -t iouring-probe app                         # native arch
docker run --rm iouring-probe                             # default seccomp profile
docker run --rm --security-opt seccomp=unconfined iouring-probe --pretty
docker build --platform linux/amd64 -t iouring-probe:amd64 app   # image used on Fargate
```

Lint (the `io-uring` crate is Linux-only, so run in a container on macOS):

```sh
docker run --rm -v "$PWD/app":/src -w /src rust:1.99-slim-trixie \
  sh -c 'rustup component add clippy rustfmt && cargo fmt --check && cargo clippy --locked -- -D warnings'
(cd infra && npm ci && npx tsc --noEmit && npx cdk synth -c imageTag=test --quiet)
```

## AWS usage

Stack name: `luciano-experiment-fargate-iouring`. All resources use that name
(or it as prefix) and carry the tag `Project=luciano-experiment-fargate-iouring`.

Prerequisites: AWS CLI v2, Docker with linux/amd64 support, `jq`.

Every script calls `sts get-caller-identity` and stops unless the account
matches `EXPECTED_ACCOUNT_ID`.

```sh
export EXPECTED_ACCOUNT_ID=123456789012 AWS_REGION=eu-west-1 AWS_PROFILE=my-profile
```

### 1. Deploy the stack

Template: [`infra/cloudformation/template.yaml`](infra/cloudformation/template.yaml).
Use the console (stack name above, acknowledge IAM capability) or the CLI:

```sh
aws cloudformation deploy \
  --stack-name luciano-experiment-fargate-iouring \
  --template-file infra/cloudformation/template.yaml \
  --capabilities CAPABILITY_IAM \
  --tags Project=luciano-experiment-fargate-iouring
```

By default it creates its own VPC. To use an existing one, add
`--parameter-overrides ExistingVpcId=vpc-... ExistingPublicSubnetIds=subnet-a,subnet-b`
(subnets must route `0.0.0.0/0` to an internet gateway).

`scripts/deploy.sh` does the same and then runs step 2.

### 2. Push the image, run, collect

```sh
scripts/push-image.sh                   # build linux/amd64, push as :latest and :<hash>
scripts/run-test.sh                     # RunTask, wait for STOPPED, save to results/
scripts/collect-results.sh <task-arn>   # re-collect a stopped task (ECS keeps it ~1 h)
scripts/redact-results.sh               # scrub account/network ids before sharing results/
```

### 3. Delete

```sh
scripts/cleanup.sh --yes
```

Stops running probe tasks, deletes the stack (ECR repo with images, log group,
cluster, task definition, role, security group, VPC), removes inactive task
definition revisions, then lists anything still carrying the project tag.
Deleting the stack from the console works too. `results/` is local and kept.

### Resources created

- ECR repository (emptied on stack delete)
- ECS cluster and task definition: 0.25 vCPU, 512 MiB, x86_64, not
  privileged, no added capabilities, no task role
- Task execution role, scoped to this repository and log group
- Log group `/ecs/luciano-experiment-fargate-iouring` (7 day retention)
- Security group with no inbound rules
- Unless an existing VPC is given: VPC `10.42.0.0/24`, 2 public subnets,
  internet gateway, no NAT gateway

No service, load balancer or NAT gateway. The task runs once and stops.

**Network security:** the task gets a public IP so it can reach ECR and
CloudWatch Logs without a NAT gateway. The security group allows no inbound
traffic. The probe makes no outbound calls apart from the ECS task metadata
endpoint.

**Cost:** a few cents per run (Fargate per-second billing, ECR storage, a
public IPv4 address while the task runs). Nothing runs between tests.

## ECS on EC2 comparison (optional)

Checks whether the Fargate result comes from the container seccomp default
rather than Fargate itself. Template:
[`infra/cloudformation/ecs-ec2.yaml`](infra/cloudformation/ecs-ec2.yaml),
stack name `luciano-experiment-ecs-ec2-iouring`.

One cluster, two `t3.micro` container instances (ECS-optimized AL2023 AMI),
same probe image and task size as Fargate:

| Instance attribute `iouring-experiment.seccomp` | Docker seccomp profile |
| --- | --- |
| `docker-default` | Docker's built-in default (blocks io_uring) |
| `custom-iouring` | Docker v25.0.18 default profile plus `io_uring_setup`, `io_uring_enter`, `io_uring_register`, set as the daemon default |

The custom instance downloads the upstream profile pinned by tag and SHA-256,
adds the three syscalls, restarts Docker and registers with ECS only if
`docker info` confirms the custom profile is active. The task is never
privileged and adds no capabilities. Instances use IMDSv2 with hop limit 1 and
a security group with no inbound rules.

```sh
aws cloudformation deploy \
  --stack-name luciano-experiment-ecs-ec2-iouring \
  --template-file infra/cloudformation/ecs-ec2.yaml \
  --capabilities CAPABILITY_IAM \
  --tags Project=luciano-experiment-ecs-ec2-iouring

STACK_NAME=luciano-experiment-ecs-ec2-iouring scripts/push-image.sh
scripts/run-test-ec2.sh docker-default
scripts/run-test-ec2.sh custom-iouring
scripts/redact-results.sh
STACK_NAME=luciano-experiment-ecs-ec2-iouring scripts/cleanup.sh --yes
```

**Cost:** two `t3.micro` instances, their 30 GiB root volumes and public IPv4
addresses: roughly USD 0.04 per hour in eu-west-1 while the stack exists.
Delete it right after the runs.

If an instance never registers, check its boot log:
`aws ec2 get-console-output --instance-id <id> --latest --output text`
(instance ids are stack outputs).

## CDK alternative

`infra/` also contains an equivalent CDK v2 stack (same stack name and
outputs). The scripts only use the CloudFormation template.

```sh
(cd infra && npm ci && npx cdk deploy)   # no bootstrap needed
```

## Known issues

- `npm audit` (CDK only) reports a moderate advisory in `minimatch` bundled by
  `aws-cdk-lib`. It is used at synth time only and not deployed.
