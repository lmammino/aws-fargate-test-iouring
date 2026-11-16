# io_uring on AWS ECS Fargate: documentation research

Research date: 2026-11-16. Scope: Linux x86_64 tasks on ECS Fargate.

Labels:

- **Documented fact**: stated by a primary source, linked inline.
- **Hypothesis**: inference not stated by any source found. To be confirmed by the PoC.

## Summary of expectations

- **Documented fact**: No AWS doc, re:Post answer, or `aws/containers-roadmap` issue
  says anything about io_uring on Fargate (ECS). Searches of
  [containers-roadmap issues for "io_uring"](https://github.com/aws/containers-roadmap/issues?q=io_uring)
  returned nothing. "io-uring" returned only
  [#2917](https://github.com/aws/containers-roadmap/issues/2917), an EKS Nitro Sandboxes
  request where a customer wants to *set* `kernel.io_uring_disabled=2`. It does not cover Fargate.
- **Documented fact**: Fargate 1.4.0 uses containerd, not Docker, and is "Based on Amazon Linux 2"
  ([changelog](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/platform-versions-changelog.html)).
  AWS does not publish the kernel version or the seccomp profile.
- **Documented fact**: Upstream default seccomp profiles block io_uring since containerd 2.0 and
  Docker 25.0. containerd 1.6 and 1.7 still allow it (see [Container seccomp defaults](#container-seccomp-defaults)).
- **Hypothesis (predicted outcome)**: The answer depends on which containerd version and
  seccomp profile the Fargate agent applies. These are not public, so the PoC is the only way to find out.
  - If Fargate applies the containerd 2.x default profile, `io_uring_setup` fails with `EPERM`.
  - If it applies a 1.6/1.7-era profile, or no profile, setup succeeds on an AL2 5.10 kernel.
  - Leaning: `EPERM`. Reason: AWS keeps patching platform revisions and upstream removed io_uring for
    container-escape risk. Confidence is low.

## Fargate constraints

### Platform version and kernel

- **Documented fact**: The `LATEST` Linux platform version is `1.4.0`. A platform version is "a combination
  of the kernel and container runtime versions". New tasks always run on the latest *revision*
  ([platform versions](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/platform-fargate.html)).
- **Documented fact**: 1.4.0 runs containerd instead of Docker, uses the Fargate container agent, and is
  "Based on Amazon Linux 2"
  ([changelog](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/platform-versions-changelog.html)).
- **Documented fact**: Linux containers use only the host kernel and kernel configuration, including sysctls
  ([task definition differences](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-tasks-services.html#fargate-task-os)).
- **Hypothesis**: The kernel is an AL2 kernel (4.14 or 5.10 line), most likely 5.10. AWS does not
  document the exact version, and revisions can change it. The PoC should log `uname -r`.
  - On 4.14, io_uring does not exist, so the syscalls return `ENOSYS`.
  - On 5.10, `kernel.io_uring_disabled` (added in 6.6) is most likely absent unless AWS backported it.

### Unsupported or restricted task definition parameters

- **Documented fact**: Not valid on Fargate: `disableNetworking`, `dnsSearchDomains`, `dnsServers`,
  `dockerSecurityOptions`, `extraHosts`, `gpu`, `ipcMode`, `links`, `placementConstraints`, `privileged`,
  `maxSwap`, `swappiness`
  ([source](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-tasks-services.html#fargate-task-parameters)).
- **Documented fact**: In `linuxParameters`, the only capability you can add is `CAP_SYS_PTRACE`.
  `devices`, `sharedMemorySize`, and `tmpfs` are not supported (same source). `CAP_SYS_ADMIN` and
  `CAP_NET_ADMIN` are restricted
  ([Fargate security considerations](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-security-considerations.html)).
- **Documented fact**: `systemControls` accepts only namespaced sysctls (IPC `kernel.msg*`/`kernel.sem`/`kernel.shm*`,
  `fs.mqueue.*`, `net.*`). "On Fargate, only namespaced `Sysctls` that exist within the container are accepted"
  ([task definition parameters](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_definition_parameters.html#container_definition_systemcontrols)).
  `kernel.io_uring_disabled` is global, not namespaced, so you cannot set it.
- **Documented fact**: Fargate uses OS default rlimits except `nofile` (65535 soft and hard). `ulimits` is supported
  and includes `memlock`
  ([task resource limits](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-tasks-services.html#fargate-resource-limits)).

### Custom seccomp

- **Documented fact**: `dockerSecurityOptions` "isn't valid for containers in tasks using the Fargate launch
  type". Even on EC2 its valid values are only `no-new-privileges`, `apparmor:PROFILE`, `label:value`, and
  `credentialspec:...`. There is no seccomp option
  ([ContainerDefinition API](https://docs.aws.amazon.com/AmazonECS/latest/APIReference/API_ContainerDefinition.html#ECS-Type-ContainerDefinition-dockerSecurityOptions)).
- **Hypothesis**: You cannot set a custom seccomp profile on Fargate. No ECS API field exists for it.
- **Hypothesis**: Fargate applies a containerd `RuntimeDefault`-like profile. AWS does not document this.
  The PoC should read `Seccomp:` and `Seccomp_filters:` from `/proc/self/status`.

## Container seccomp defaults

### containerd (`contrib/seccomp/seccomp_default.go`)

- **Documented fact**: [containerd#4493](https://github.com/containerd/containerd/pull/4493)
  ("seccomp: allow io-uring related system calls", merged 2020-08-25) added the three syscalls.
- **Documented fact**: [containerd#9320](https://github.com/containerd/containerd/pull/9320)
  ("Update RuntimeDefault seccomp profile to disallow io_uring related syscalls", merged 2023-11-02 into `main`,
  commit [a48ddf4](https://github.com/containerd/containerd/commit/a48ddf4a208b24eadea82f0eac62e236f2acf004)) removed them.
  The PR cites [Google kCTF findings](https://security.googleblog.com/2023/06/learnings-from-kctf-vrps-42-linux.html)
  on container breakouts. Tracking issue: [containerd#9048](https://github.com/containerd/containerd/issues/9048).
- **Documented fact**: Status by branch (checked 2026-11-16):
  - [`release/1.6`](https://github.com/containerd/containerd/blob/release/1.6/contrib/seccomp/seccomp_default.go)
    and [`release/1.7`](https://github.com/containerd/containerd/blob/release/1.7/contrib/seccomp/seccomp_default.go)
    still **allow** `io_uring_enter`, `io_uring_register`, `io_uring_setup`.
  - [`release/2.0`](https://github.com/containerd/containerd/blob/release/2.0/contrib/seccomp/seccomp_default.go)
    and [`main`](https://github.com/containerd/containerd/blob/main/contrib/seccomp/seccomp_default.go) **do not**.
- **Documented fact**: The default action is `specs.ActErrno` with no `DefaultErrnoRet` set
  ([source](https://github.com/containerd/containerd/blob/main/contrib/seccomp/seccomp_default.go)).
  The OCI runtime spec defines `defaultErrnoRet` default as `EPERM`
  ([runtime-spec config-linux seccomp](https://github.com/opencontainers/runtime-spec/blob/main/config-linux.md#seccomp)).

### Docker / moby (`profiles/seccomp/default.json`)

- **Documented fact**: [moby#39415](https://github.com/moby/moby/pull/39415) added io_uring to the allowlist.
  [moby#46762](https://github.com/moby/moby/pull/46762) ("seccomp: block io_uring_* syscalls in default profile",
  merged 2023-11-02, milestone 25.0.0) removed them to sync with containerd.
- **Documented fact**: The [24.0 profile](https://github.com/moby/moby/blob/24.0/profiles/seccomp/default.json)
  contains io_uring. The [25.0 profile](https://github.com/moby/moby/blob/25.0/profiles/seccomp/default.json)
  does not. The current [moby/profiles default.json](https://github.com/moby/profiles/blob/main/seccomp/default.json)
  has `defaultAction: SCMP_ACT_ERRNO` and `defaultErrnoRet: 1` (`EPERM`).
- **Documented fact**: [moby#47532](https://github.com/moby/moby/issues/47532) is a user request to allow io_uring
  again under 25.0.
- **Hypothesis**: Blocked io_uring calls return `EPERM`, not `ENOSYS`. runc's ENOSYS stub only covers syscall numbers
  greater than the highest number in the profile
  ([runc patchbpf](https://github.com/opencontainers/runc/blob/main/libcontainer/seccomp/patchbpf/enosys_linux.go)).
  The profile lists syscalls above 427 (for example `clone3` 435), so 425 to 427 get the default `EPERM`.

### ECS on EC2 (comparison baseline)

- **Documented fact**: ECS optimized AMI release
  [20261113](https://github.com/aws/amazon-ecs-ami/releases/tag/20261113): AL2023 x86_64
  (`al2023-ami-ecs-hvm-2023.0.20261113-kernel-6.1-x86_64`) ships Docker 25.0.18, containerd 2.3.7, runc 1.4.3.
  The AL2 variants list the same Docker and containerd versions.
- **Hypothesis**: ECS on EC2 runs containers through Docker with its default profile, because ECS
  offers no seccomp override. So io_uring is blocked with `EPERM` on current ECS AMIs.

## Kernel requirements

- **Documented fact**: x86_64 syscall numbers: `io_uring_setup` 425, `io_uring_enter` 426, `io_uring_register` 427,
  `mmap` 9, `munmap` 11, `close` 3
  ([syscall_64.tbl](https://github.com/torvalds/linux/blob/master/arch/x86/entry/syscalls/syscall_64.tbl)).
- **Documented fact**: io_uring needs `CONFIG_IO_URING`
  ([init/Kconfig](https://github.com/torvalds/linux/blob/master/init/Kconfig)). It first shipped in Linux 5.1
  ([io_uring(7)](https://man7.org/linux/man-pages/man7/io_uring.7.html)).
- **Hypothesis**: With `CONFIG_IO_URING=n`, or on a kernel older than 5.1, the syscalls return `ENOSYS`
  (standard `sys_ni_syscall` behavior,
  [kernel/sys_ni.c](https://github.com/torvalds/linux/blob/master/kernel/sys_ni.c)).
- **Documented fact**: `kernel.io_uring_disabled`
  ([sysctl docs](https://docs.kernel.org/admin-guide/sysctl/kernel.html#io-uring-disabled)):
  - `0` (default): all processes can create io_uring instances.
  - `1`: `io_uring_setup()` fails with `-EPERM` unless the caller has `CAP_SYS_ADMIN` or is in `kernel.io_uring_group`.
  - `2`: `io_uring_setup()` always fails with `-EPERM`.
  - `io_uring_group` defaults to `-1`, meaning only `CAP_SYS_ADMIN` qualifies.
  - Added by commit [76d3ccec](https://github.com/torvalds/linux/commit/76d3ccecfa186af3120e206d62f03db1a94a535f)
    (2023-09-05), merged for 6.6. The same errors are listed in
    [io_uring_setup.2](https://github.com/axboe/liburing/blob/master/man/io_uring_setup.2).
- **Documented fact**: Basic ring setup needs no capability. The only privilege errors in
  [io_uring_setup(2)](https://man7.org/linux/man-pages/man2/io_uring_setup.2.html) are for SQPOLL and the sysctl.
- **Documented fact**: SQPOLL privileges by kernel version
  ([io_uring_setup(2)](https://man7.org/linux/man-pages/man2/io_uring_setup.2.html)):
  - Before 5.11: requires root.
  - 5.11 and 5.12: allowed with `CAP_SYS_NICE`.
  - 5.13 and later: needs no privilege. Some older stable kernels have the relaxation backported.
  - Insufficient privilege returns `EPERM`.
- **Documented fact**: Ring memory counted against `RLIMIT_MEMLOCK` on 5.11 and earlier. Newer kernels use it
  only for registered buffers ([liburing README](https://github.com/axboe/liburing/blob/master/README)).
  Registered buffers are charged to `RLIMIT_MEMLOCK` unless the process has `CAP_IPC_LOCK`. If the limit is
  exceeded, the call returns `ENOMEM`
  ([io_uring_register.2](https://github.com/axboe/liburing/blob/master/man/io_uring_register.2)).
- **Hypothesis**: On a 5.10 Fargate kernel, the default `memlock` rlimit could make large rings fail with
  `ENOMEM`. Use small rings (for example 8 entries) or raise `memlock` via `ulimits`.

### Errno summary

| Blocking layer | Errno | Label |
| --- | --- | --- |
| Docker/containerd default seccomp (`SCMP_ACT_ERRNO`, errno 1) | `EPERM` | Documented fact ([moby profile](https://github.com/moby/profiles/blob/main/seccomp/default.json)) |
| `io_uring_disabled=2` | `EPERM` | Documented fact ([sysctl docs](https://docs.kernel.org/admin-guide/sysctl/kernel.html#io-uring-disabled)) |
| `io_uring_disabled=1`, caller not in group and no `CAP_SYS_ADMIN` | `EPERM` | Documented fact (same) |
| `CONFIG_IO_URING=n` or kernel < 5.1 | `ENOSYS` | Hypothesis ([sys_ni.c](https://github.com/torvalds/linux/blob/master/kernel/sys_ni.c)) |
| SQPOLL without privilege on < 5.13 | `EPERM` | Documented fact ([io_uring_setup(2)](https://man7.org/linux/man-pages/man2/io_uring_setup.2.html)) |
| `RLIMIT_MEMLOCK` exceeded | `ENOMEM` | Documented fact ([io_uring_register.2](https://github.com/axboe/liburing/blob/master/man/io_uring_register.2)) |

Seccomp and the sysctl both return `EPERM`, so the errno alone does not tell them apart. The PoC can check
`/proc/sys/kernel/io_uring_disabled` (if present) and `Seccomp:` in `/proc/self/status` to tell which one blocked the call.

**PoC approach (design decision, not a source claim):** the probe also calls each syscall with arguments
the kernel rejects cheaply (`io_uring_setup(0, ...)`, `io_uring_enter(-1, ...)`, `io_uring_register(-1, ...)`).
The sysctl only gates `io_uring_setup`, and a seccomp filter runs before any argument check. So:

- `EBADF`/`EINVAL` from all three: the kernel handled the calls.
- `EPERM` from `enter`/`register` too: a seccomp filter (the sysctl cannot cause this).
- `EPERM` only from `setup`, with `EBADF`/`EINVAL` from the others: kernel policy (`io_uring_disabled`).
- `ENOSYS` while `/proc/kallsyms` lists `io_uring_setup`: something between the process and the kernel
  does not pass the syscall through (see below).

**Local observation (2026-11-16, OrbStack on Apple Silicon, kernel 7.2.12):** an x86_64 image run under
Rosetta emulation got `ENOSYS` from all three syscalls, even with `--security-opt seccomp=unconfined`,
while `/proc/kallsyms` listed `io_uring_setup`. The same image built for arm64 got `EPERM` under the
default Docker 29.8.2 profile and passed every test with `seccomp=unconfined`. Local x86_64 runs on
Apple Silicon therefore cannot test io_uring.

## Minimum syscall set

- **Documented fact**: The minimal flow in
  [io_uring_setup(2)](https://man7.org/linux/man-pages/man2/io_uring_setup.2.html) and
  [io_uring(7)](https://man7.org/linux/man-pages/man7/io_uring.7.html):
  1. `io_uring_setup(entries, &params)` returns a ring fd.
  2. `mmap` the SQ ring (`IORING_OFF_SQ_RING`), the CQ ring (`IORING_OFF_CQ_RING`), and the SQE array
     (`IORING_OFF_SQES`). With `IORING_FEAT_SINGLE_MMAP` (5.4+), the SQ and CQ rings share one mapping,
     so there are 2 `mmap` calls.
  3. `io_uring_enter(fd, to_submit, min_complete, IORING_ENTER_GETEVENTS, ...)` to submit and wait.
  4. Optional: `io_uring_register` for fixed buffers, files, or probing (`IORING_REGISTER_PROBE`, 5.6+,
     [io_uring_register.2](https://github.com/axboe/liburing/blob/master/man/io_uring_register.2)).
  5. `munmap` and `close(fd)` to tear down.
- **Hypothesis**: A `IORING_OP_NOP` round trip is enough to prove the ring works. Other opcodes
  (for example `IORING_OP_READ`, 5.6+) test real I/O.

## Expected outcomes matrix (hypotheses)

All rows are **Hypotheses**.

| Environment | Kernel | Seccomp | Expected `io_uring_setup` result |
| --- | --- | --- | --- |
| Fargate 1.4.0, containerd 2.x default profile | AL2 5.10 | io_uring absent | `EPERM` |
| Fargate 1.4.0, containerd 1.6/1.7-era profile | AL2 5.10 | io_uring allowed | Success. NOP round trip works. SQPOLL likely works (5.13+ rule backported?) |
| Fargate 1.4.0, no seccomp profile | AL2 5.10 | none | Success |
| Fargate on an AL2 4.14 kernel | 4.14 | any | `ENOSYS` (or `EPERM` if seccomp filters first) |
| Fargate with `io_uring_disabled=2` | 6.6+ or backport | any | `EPERM` |
| ECS on EC2, AL2023 AMI 20261113 (Docker 25.0.18) | 6.1 | Docker default | `EPERM` |
| Local `docker run` with Docker >= 25.0 | host | Docker default | `EPERM`; `--security-opt seccomp=unconfined` → success |

Diagnostics the PoC should log: `uname -r`, `/proc/self/status` (`Seccomp`, `Seccomp_filters`, `CapEff`),
`/proc/sys/kernel/io_uring_disabled`, `/proc/sys/kernel/io_uring_group`, `ulimit -l`, and the raw errno of each syscall.

## Rust crates

- **Documented fact**: [`io-uring`](https://crates.io/crates/io-uring) latest is **0.7.16** (published 2026-11-09),
  repo [tokio-rs/io-uring](https://github.com/tokio-rs/io-uring).
- **Documented fact**: [`tokio-uring`](https://crates.io/crates/tokio-uring) latest is **0.5.0** (2024-05-27).
  The [repo](https://github.com/tokio-rs/tokio-uring) is not archived. Last push was 2025-07-07, and the latest
  GitHub release tag is v0.4.0 (2022-11-05).
- **Hypothesis**: `tokio-uring` is low-activity. For a minimal probe, use `io-uring` directly, or raw `libc::syscall`
  to see exact errnos without crate-side feature probing.

## References

- AWS Fargate platform versions: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/platform-fargate.html
- Fargate Linux platform changelog: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/platform-versions-changelog.html
- Fargate task definition differences: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-tasks-services.html
- Fargate security considerations: https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-security-considerations.html
- Task definition parameters (Fargate): https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_definition_parameters.html
- ECS ContainerDefinition API: https://docs.aws.amazon.com/AmazonECS/latest/APIReference/API_ContainerDefinition.html
- ECS optimized AMI release 20261113: https://github.com/aws/amazon-ecs-ami/releases/tag/20261113
- containers-roadmap #2917: https://github.com/aws/containers-roadmap/issues/2917
- containerd #4493: https://github.com/containerd/containerd/pull/4493
- containerd #9320: https://github.com/containerd/containerd/pull/9320
- containerd #9048: https://github.com/containerd/containerd/issues/9048
- containerd seccomp default (main): https://github.com/containerd/containerd/blob/main/contrib/seccomp/seccomp_default.go
- moby #39415: https://github.com/moby/moby/pull/39415
- moby #46762: https://github.com/moby/moby/pull/46762
- moby #47532: https://github.com/moby/moby/issues/47532
- moby/profiles default.json: https://github.com/moby/profiles/blob/main/seccomp/default.json
- runc ENOSYS patch: https://github.com/opencontainers/runc/blob/main/libcontainer/seccomp/patchbpf/enosys_linux.go
- OCI runtime spec, seccomp: https://github.com/opencontainers/runtime-spec/blob/main/config-linux.md#seccomp
- Kernel sysctl docs: https://docs.kernel.org/admin-guide/sysctl/kernel.html#io-uring-disabled
- io_uring_disabled commit: https://github.com/torvalds/linux/commit/76d3ccecfa186af3120e206d62f03db1a94a535f
- x86_64 syscall table: https://github.com/torvalds/linux/blob/master/arch/x86/entry/syscalls/syscall_64.tbl
- io_uring_setup(2): https://man7.org/linux/man-pages/man2/io_uring_setup.2.html
- io_uring(7): https://man7.org/linux/man-pages/man7/io_uring.7.html
- liburing man pages and README: https://github.com/axboe/liburing
- Google kCTF VRP learnings: https://security.googleblog.com/2023/06/learnings-from-kctf-vrps-42-linux.html
- io-uring crate: https://crates.io/crates/io-uring
- tokio-uring crate: https://crates.io/crates/tokio-uring
