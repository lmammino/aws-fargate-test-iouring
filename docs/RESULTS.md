# Results: io_uring on AWS ECS Fargate (and ECS on EC2)

Run date: 2026-11-16. Two runs, identical results. Evidence (`task.json`,
`log-events.json`, `probe.log`, `result.json`, `summary.json`):

- Run 1: [`results/20261116T110509Z-4db7c61a9d62471b85664d25caa6a3e4/`](../results/20261116T110509Z-4db7c61a9d62471b85664d25caa6a3e4/)
- Run 2: [`results/20261116T110827Z-7885d64dc0ca41d188917ba4912f0d86/`](../results/20261116T110827Z-7885d64dc0ca41d188917ba4912f0d86/)

ECS on EC2 comparison (section 7):

- Docker default profile: [`results/20261116T112603Z-ec2-docker-default-e282860901f54543bcb4f7459c8023d7/`](../results/20261116T112603Z-ec2-docker-default-e282860901f54543bcb4f7459c8023d7/)
- Default profile plus io_uring: [`results/20261116T112616Z-ec2-custom-iouring-dbfd7827be324af7810b4c41cb770420/`](../results/20261116T112616Z-ec2-custom-iouring-dbfd7827be324af7810b4c41cb770420/)

Account ids, subnet and ENI ids, MAC and private IP addresses were replaced
with placeholders by `scripts/redact-results.sh`. Nothing else was changed.
Sections 2 to 4 quote Fargate run 1.

## 1. Executive summary

**Does io_uring work on AWS ECS Fargate in the tested environment? No.**

Classification: **Unavailable or blocked.**

The kernel has io_uring compiled in, but all three io_uring syscalls
(`io_uring_setup`, `io_uring_enter`, `io_uring_register`) fail with `EPERM`
before the kernel looks at their arguments. That pattern, together with
`Seccomp: 2` and one active filter, points to the container's seccomp filter.
Task definitions on Fargate offer no setting that changes this.

**Does it work on ECS on EC2? Only with a custom seccomp profile.** With the
same image on an ECS-optimized AL2023 instance, Docker's default profile gives
the same `EPERM` result as Fargate. Adding only the three io_uring syscalls to
that profile (set as the Docker daemon default) makes every test pass,
including SQPOLL and registered buffers and files. The task stays
unprivileged, non-root and without added capabilities. Changing only the
seccomp profile flips the outcome, which confirms seccomp as the blocker on
ECS on EC2 and strongly supports it as the blocker on Fargate.

## 2. Test environment

| Item | Value | Source |
| --- | --- | --- |
| Region / AZ | eu-west-1 / eu-west-1b | `task.json` |
| Launch type | FARGATE | `task.json`, task metadata |
| Platform version | 1.4.0 (requested `LATEST`) | `task.json` `platformVersion` |
| Task size | 0.25 vCPU, 512 MiB, 20 GiB ephemeral | `task.json` |
| Kernel | `6.1.189 #1 SMP PREEMPT_DYNAMIC Fri Oct 30 22:31:59 UTC 2026` | `uname` |
| Kernel build string | `x86_64-bottlerocket-linux-gnu-gcc (Buildroot 2025.05.1) 13.4.0` | `/proc/version` |
| CPU | Intel Xeon Platinum 8259CL @ 2.50GHz, 2 visible CPUs | `/proc/cpuinfo` |
| Architecture | x86_64 | probe |
| Container image | distroless `cc-debian13:nonroot`, digest `sha256:09492b99f2bf...4d5f` | `task.json` |
| User | uid/gid 65532 (non-root) | probe |
| Effective capabilities | none (`CapEff 0000000000000000`) | `/proc/self/status` |
| Bounding set | `00000000a80425fb` (default container set, no `CAP_SYS_ADMIN`) | `/proc/self/status` |
| Seccomp | mode 2 (filter), `Seccomp_filters: 1` | `/proc/self/status` |
| `NoNewPrivs` | 0 | `/proc/self/status` |
| `kernel.io_uring_disabled` | not present | `/proc/sys/kernel/` |
| `RLIMIT_MEMLOCK` | 8 MiB soft and hard | `getrlimit` |
| Task definition security | not privileged, no added capabilities, no seccomp options (not supported on Fargate) | `infra/cloudformation/template.yaml` |
| Rust | rustc 1.99.1 | probe build |
| `io-uring` crate | 0.7.16 | `Cargo.lock` |

Task lifecycle: created 11:04:24, started 11:04:38, stopped 11:05:01 UTC.
`stopCode: EssentialContainerExited`, container `exitCode: 11`. The image
pulled and logs were delivered, so infrastructure problems are ruled out.

## 3. Test results

| Test | Status | errno | Notes |
| --- | --- | --- | --- |
| environment | collected | | See section 2 |
| syscall_reachability: `io_uring_setup(entries=0)` | blocked | 1 `EPERM` | The kernel would return `EINVAL` for 0 entries |
| syscall_reachability: `io_uring_enter(fd=-1)` | blocked | 1 `EPERM` | The kernel would return `EBADF` |
| syscall_reachability: `io_uring_register(fd=-1)` | blocked | 1 `EPERM` | The kernel would return `EBADF` |
| direct_syscall (`io_uring_setup(8, {0})`, nr 425) | failed | 1 `EPERM` | |
| ring_initialization (`IoUring::new(8)`) | failed | 1 `EPERM` | Same result through the crate |
| nop | skipped | | Ring initialization failed |
| file_write | skipped | | Ring initialization failed |
| file_read | skipped | | Ring initialization failed |
| opcode_probe | skipped | | Ring initialization failed |
| registered_buffers | skipped | | Ring initialization failed |
| registered_files | skipped | | Ring initialization failed |
| sqpoll | skipped | | Ring initialization failed |
| conventional_io (`std::fs` round trip) | passed | | 40 bytes, data verified |

Probe summary: `classification: unavailable`, exit code 11.

## 4. Evidence

Probe log from CloudWatch (`/ecs/luciano-experiment-fargate-iouring`, stream
`probe/probe/4db7c61a9d62471b85664d25caa6a3e4`). The environment JSON line is
shortened here; the full text is in `probe.log`.

```text
[iouring-probe] starting io_uring probe v0.1.0
[iouring-probe] environment: {"architecture":"x86_64","cpu_count":2,"cpu_model":"Intel(R) Xeon(R) Platinum 8259CL CPU @ 2.50GHz",...,"effective_capabilities":{"hex":"0000000000000000","names":[]},...,"io_uring_disabled":null,...,"kallsyms_io_uring_setup":true,"kernel":{"machine":"x86_64","release":"6.1.189",...},...,"rlimit_memlock":{"hard":8388608,"soft":8388608},"seccomp_mode":2,"seccomp_mode_meaning":"filter","uid":65532}
[iouring-probe] syscall reachability: {...,"io_uring_enter(fd=-1)":{"errno":1,"errno_name":"EPERM",...},"io_uring_register(fd=-1)":{"errno":1,"errno_name":"EPERM",...},"io_uring_setup(entries=0)":{"errno":1,"errno_name":"EPERM",...}}
[iouring-probe] test direct_syscall: failed (errno 1 EPERM: Operation not permitted (os error 1))
[iouring-probe] test ring_initialization: failed (errno 1 EPERM: Operation not permitted (os error 1))
[iouring-probe] test nop: skipped (ring initialization failed)
[iouring-probe] test file_write: skipped (ring initialization failed)
[iouring-probe] test file_read: skipped (ring initialization failed)
[iouring-probe] test opcode_probe: skipped (ring initialization failed)
[iouring-probe] test registered_buffers: skipped (ring initialization failed)
[iouring-probe] test registered_files: skipped (ring initialization failed)
[iouring-probe] test sqpoll: skipped (ring initialization failed)
[iouring-probe] test conventional_io: passed
[iouring-probe] classification: unavailable (io_uring_setup failed both via raw syscall and via the io-uring crate)
```

ECS task (`summary.json`):

```json
{
  "lastStatus": "STOPPED",
  "stopCode": "EssentialContainerExited",
  "stoppedReason": "Essential container in task exited",
  "platformVersion": "1.4.0",
  "platformFamily": "Linux",
  "container": { "exitCode": 11, "reason": null }
}
```

Excerpt of the JSON report (`result.json`):

```json
"tests": {
  "direct_syscall": {
    "status": "failed", "stage": "io_uring_setup",
    "errno": 1, "errno_name": "EPERM", "error": "Operation not permitted (os error 1)",
    "request": { "entries": 8, "flags": 0, "syscall_nr": 425 }
  },
  "ring_initialization": {
    "status": "failed", "stage": "IoUring::new", "queue_depth": 8,
    "errno": 1, "errno_name": "EPERM", "error": "Operation not permitted (os error 1)"
  },
  "conventional_io": { "status": "passed", "bytes_read": 40, "data_verified": true }
}
```

Local control runs of the same probe (arm64, OrbStack kernel 7.2.12, Docker
29.8.2) are described in [RESEARCH.md](RESEARCH.md): `EPERM` on all three
syscalls with the default Docker profile, all tests passing with
`--security-opt seccomp=unconfined`. Same pattern as Fargate in the first case.

## 5. Technical interpretation

| Possible cause | Verdict | Evidence |
| --- | --- | --- |
| Kernel lacks io_uring (`CONFIG_IO_URING=n`, too old) | **Ruled out** | Kernel 6.1. `/proc/kallsyms` lists `io_uring_setup`. A missing syscall returns `ENOSYS`, not `EPERM`. |
| Kernel policy (`kernel.io_uring_disabled`) | **Ruled out** | The sysctl file does not exist (added in 6.6, apparently not backported to this kernel). It would also only affect `io_uring_setup`, not `enter` or `register`. |
| Linux capabilities | **Not the cause** | Basic ring setup needs no capability. `CapEff` is empty because the probe runs as non-root, but nothing in io_uring setup checks capabilities when the sysctl is absent. |
| Unsupported operation or flags | **Ruled out** | The calls fail the same way with valid (`entries=8`) and invalid arguments. No operation was ever submitted. |
| Seccomp filter | **Most likely cause** (confirmed on ECS on EC2, see section 7) | Seccomp mode 2 with one filter. `io_uring_enter(-1, ...)` and `io_uring_register(-1, ...)` returned `EPERM`. The kernel would have returned `EBADF` because no file descriptor -1 exists, so something rejected the calls before the kernel ran them. A seccomp filter is the mechanism that does that. `EPERM` is the default errno of the containerd and Docker profiles, which removed io_uring in containerd 2.0 and Docker 25.0. |

What the evidence does not establish:

- We cannot read the filter contents from inside the container. So it is not
  proven which profile Fargate uses, only that it blocks these three syscalls
  with `EPERM`.
- AWS documents platform 1.4.0 as "based on Amazon Linux 2". The kernel here
  is 6.1 and was built with a Bottlerocket toolchain. This suggests the host
  stack differs from what the docs describe (hypothesis). It does not affect
  the conclusion.
- The probe ran as non-root. Running as root would add the default
  capabilities but not `CAP_SYS_ADMIN`, which Fargate does not allow. The
  default container profiles do not allow io_uring based on capabilities, so
  root is not expected to change the result. This was not tested.

## 6. Practical recommendations

- **io_uring-based runtimes on Fargate:** not usable. Runtimes such as
  `tokio-uring`, which require io_uring and have no fallback, fail at startup
  when ring creation returns `EPERM`. No Fargate task definition setting can
  change this: `privileged`, custom seccomp profiles and `CAP_SYS_ADMIN` are
  all unavailable on Fargate.
- **Recommended on Fargate:** conventional Tokio (epoll for networking, a
  blocking thread pool for files). It needs no special kernel access, and the
  probe's conventional I/O control test passed.
- **If io_uring is required:** run the workload where you control the seccomp
  profile.
  - ECS on EC2 (**tested, works**): ECS has no seccomp setting in task
    definitions, so set a custom default profile on the Docker daemon
    (Docker's default plus the three io_uring syscalls) through the instance
    user data, as `infra/cloudformation/ecs-ec2.yaml` does. This affects every
    container on the instance, so use dedicated instances (a separate capacity
    provider) for io_uring workloads. `privileged` would also work but removes
    far more isolation than needed (not tested, not recommended).
  - EKS on EC2 (not tested): Kubernetes supports per-pod `seccompProfile`
    (`Localhost` profile that adds the three io_uring syscalls), which limits
    the change to one workload.
  - Note that upstream removed io_uring from the default profiles because of
    container-escape risk (see RESEARCH.md). Enabling it is a deliberate
    security trade-off.
- **Libraries that can use io_uring when available** should detect `EPERM` or
  `ENOSYS` from `io_uring_setup` at startup and fall back to epoll. The probe's
  `direct_syscall` test shows a cheap way to detect this.

## 7. ECS on EC2 comparison

Goal: tell apart "blocked by Fargate" from "blocked by the container seccomp
default". Template: `infra/cloudformation/ecs-ec2.yaml` (stack
`luciano-experiment-ecs-ec2-iouring`). One cluster, two `t3.micro` container
instances on the ECS-optimized AL2023 AMI, same task definition shape as
Fargate (0.25 vCPU, 512 MiB, x86_64, not privileged, no added capabilities,
bridge network mode), same image digest `sha256:09492b99...4d5f`.

| Instance | Docker seccomp profile |
| --- | --- |
| `docker-default` | Docker's built-in default |
| `custom-iouring` | [moby v25.0.18 `default.json`](https://github.com/moby/moby/blob/v25.0.18/profiles/seccomp/default.json) (SHA-256 `9c1025c8...9c76`) plus one rule allowing `io_uring_setup`, `io_uring_enter`, `io_uring_register`, set as the daemon's `seccomp-profile` |

### Results

| | Fargate (runs 1 and 2) | EC2, Docker default | EC2, default + io_uring |
| --- | --- | --- | --- |
| Kernel | 6.1.189 (Bottlerocket toolchain) | 6.1.190-237.442.amzn2023 | 6.1.190-237.442.amzn2023 |
| Seccomp | mode 2, 1 filter | mode 2, 1 filter | mode 2, 1 filter |
| `CapEff` / uid | none / 65532 | none / 65532 | none / 65532 |
| `kernel.io_uring_disabled` | not present | not present | not present |
| `RLIMIT_MEMLOCK` | 8 MiB | unlimited | unlimited |
| `io_uring_setup(entries=0)` | `EPERM` | `EPERM` | `EINVAL` (kernel) |
| `io_uring_enter(fd=-1)` | `EPERM` | `EPERM` | `EBADF` (kernel) |
| `io_uring_register(fd=-1)` | `EPERM` | `EPERM` | `EBADF` (kernel) |
| direct_syscall, ring_initialization | failed `EPERM` | failed `EPERM` | passed (8 SQ / 16 CQ entries, features `0x1fff`) |
| nop, file_write, file_read | skipped | skipped | passed, data verified |
| opcode_probe | skipped | skipped | passed, 49 opcodes |
| registered_buffers, registered_files, sqpoll | skipped | skipped | passed |
| conventional_io | passed | passed | passed |
| Classification, exit code | unavailable, 11 | unavailable, 11 | **supported, 0** |

Both EC2 tasks ran in eu-west-1a at 11:25:57 and 11:26:09 UTC and stopped with
`EssentialContainerExited`.

### Interpretation

- The Docker default profile on ECS on EC2 reproduces the Fargate result
  exactly, errno for errno.
- The only difference between the two EC2 instances is the seccomp profile.
  Allowing the three syscalls turns `EPERM` into the kernel's own argument
  errors (`EINVAL`, `EBADF`), exactly as the reachability check predicts, and
  all io_uring tests pass. Seccomp is the blocker on ECS on EC2.
- Fargate's kernel differs (6.1.189, Bottlerocket toolchain), so the EC2 run
  does not prove what Fargate's filter contains. It does show that the
  Fargate behavior is identical to a standard container seccomp profile without
  io_uring, which is the simplest explanation.
- `RLIMIT_MEMLOCK` is 8 MiB on Fargate and unlimited on EC2. It played no role
  here (rings were never created on Fargate), but it would limit registered
  buffers on Fargate if io_uring were ever allowed.

## 8. Reproducibility and cleanup

| | Run 1 | Run 2 |
| --- | --- | --- |
| Task started (UTC) | 11:04:38 | 11:08:00 |
| AZ | eu-west-1b | eu-west-1b |
| Platform version | 1.4.0 | 1.4.0 |
| Kernel | 6.1.189 | 6.1.189 |
| CPU | Intel Xeon Platinum 8259CL | AMD EPYC 7571 |
| Image digest | `sha256:09492b99...4d5f` | same |
| Seccomp | mode 2, 1 filter | mode 2, 1 filter |
| setup / enter / register | `EPERM` / `EPERM` / `EPERM` | `EPERM` / `EPERM` / `EPERM` |
| conventional_io | passed | passed |
| Classification, exit code | unavailable, 11 | unavailable, 11 |

The different CPUs show that the runs landed on different hosts.

- Re-run with `scripts/run-test.sh`. Each run writes a new directory under `results/`.
- Delete everything with `scripts/cleanup.sh --yes` (or delete the stack
  `luciano-experiment-fargate-iouring`).

### Cleanup verification (2026-11-16)

The stack was deleted from the console after both runs. State checked with the AWS CLI afterwards:

| Resource | State | Verdict |
| --- | --- | --- |
| CloudFormation stack | does not exist | deleted |
| ECS cluster | `INACTIVE` | deleted (ECS keeps the record for a while) |
| Task definition `:1` | `DELETE_IN_PROGRESS` | being deleted by ECS |
| ECR repository and images | `RepositoryNotFoundException` | deleted |
| Log group `/ecs/luciano-experiment-fargate-iouring` | none | deleted |
| VPC tagged with the project | none | deleted |

The ECS on EC2 stack was deleted with
`STACK_NAME=luciano-experiment-ecs-ec2-iouring scripts/cleanup.sh --yes`,
which also deleted task definition revision `:1`. Checked afterwards:

| Resource | State | Verdict |
| --- | --- | --- |
| CloudFormation stack | deleted (`stack-delete-complete`) | deleted |
| Both EC2 container instances | `terminated` | deleted |
| Both EBS root volumes | `InvalidVolume.NotFound`, no volumes with the project tag | deleted |
| Task definition `:1` | deleted by the script | deleted |

For both stacks, the tagging API still listed stopped tasks, the inactive
cluster, the deleted task definition and (for EC2) the terminated instances
and deleted volumes for a while after deletion. Those are stale records that
AWS removes on its own, and they have no cost.
