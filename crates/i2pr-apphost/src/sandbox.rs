//! Linux x86_64/aarch64 secured launch enforcement.
//!
//! The v1 profile is deliberately limited to static ELF executables. That lets
//! Landlock grant exactly the immutable package tree and persistent app data
//! without granting `/usr`, a loader cache, or the host's general library tree.

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use i2pr_app_proto::{SandboxAttestation, SandboxProperty};
use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr, RulesetStatus,
};
use rustix::process::{Resource, Rlimit, setrlimit};

use crate::{ApphostError, ResolvedLaunch};

const MAX_PROGRAM_HEADERS: u16 = 1024;
const MAX_PROGRAM_HEADER_BYTES: u64 = 128 * 1024;

/// Install every Linux enforcement layer, failing before readiness if one is
/// absent or only partially enforced.
pub(crate) fn install(
    launch: &ResolvedLaunch,
    memory_bytes: u64,
    open_files: u32,
) -> Result<SandboxAttestation, ApphostError> {
    ensure_static_elf(&launch.program)?;
    if memory_bytes == 0 || open_files == 0 {
        return Err(ApphostError::SandboxUnavailable);
    }

    // Limits are hard: soft and hard values are identical, and seccomp omits
    // setrlimit/prlimit so the application cannot raise them after exec.
    setrlimit(
        Resource::As,
        Rlimit {
            current: Some(memory_bytes),
            maximum: Some(memory_bytes),
        },
    )
    .map_err(|_| ApphostError::SandboxSetup)?;
    let files = u64::from(open_files);
    setrlimit(
        Resource::Nofile,
        Rlimit {
            current: Some(files),
            maximum: Some(files),
        },
    )
    .map_err(|_| ApphostError::SandboxSetup)?;

    let read_only = AccessFs::from_read(ABI::V3);
    let app_data = (AccessFs::from_write(ABI::V3) | AccessFs::ReadFile | AccessFs::ReadDir)
        & !AccessFs::Execute;
    let status = Ruleset::default()
        .handle_access(AccessFs::from_all(ABI::V3))
        .map_err(|_| ApphostError::SandboxUnavailable)?
        .create()
        .map_err(|_| ApphostError::SandboxUnavailable)?
        .set_compatibility(CompatLevel::HardRequirement)
        .add_rule(PathBeneath::new(
            PathFd::new(&launch.root).map_err(|_| ApphostError::SandboxSetup)?,
            read_only,
        ))
        .map_err(|_| ApphostError::SandboxSetup)?
        .add_rule(PathBeneath::new(
            PathFd::new(&launch.data_root).map_err(|_| ApphostError::SandboxSetup)?,
            app_data,
        ))
        .map_err(|_| ApphostError::SandboxSetup)?
        .restrict_self()
        .map_err(|_| ApphostError::SandboxUnavailable)?;
    if status.ruleset != RulesetStatus::FullyEnforced || !status.no_new_privs {
        return Err(ApphostError::SandboxUnavailable);
    }

    let program = syscall_filter();
    bux_seccomp::install(&program).map_err(|_| ApphostError::SandboxUnavailable)?;

    let attestation = SandboxAttestation {
        backend_kind: "linux-landlock-seccomp-static-elf".to_owned(),
        backend_version: "1".to_owned(),
        evidence_generation: 1,
        properties: vec![
            SandboxProperty::DirectNetworkDenied,
            SandboxProperty::LoopbackDenied,
            SandboxProperty::PrivateFilesystem,
            SandboxProperty::ProcessInspectionContained,
            SandboxProperty::ChildTreeContained,
            SandboxProperty::ResourceLimitsInstalled,
            SandboxProperty::EnvironmentSanitized,
            SandboxProperty::BrokerChannelInstalled,
        ],
    };
    attestation
        .validate_secured()
        .map_err(|_| ApphostError::SandboxSetup)?;
    Ok(attestation)
}

/// Static ELFs are the first supported executable model. Reject malformed,
/// foreign-architecture, and interpreter-bearing files before installing rules.
fn ensure_static_elf(path: &Path) -> Result<(), ApphostError> {
    let mut file = File::open(path).map_err(|_| ApphostError::SandboxSetup)?;
    let mut header = [0_u8; 64];
    file.read_exact(&mut header)
        .map_err(|_| ApphostError::SandboxUnavailable)?;
    if &header[..4] != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return Err(ApphostError::SandboxUnavailable);
    }
    let machine = u16::from_le_bytes([header[18], header[19]]);
    #[cfg(target_arch = "x86_64")]
    if machine != 62 {
        return Err(ApphostError::SandboxUnavailable);
    }
    #[cfg(target_arch = "aarch64")]
    if machine != 183 {
        return Err(ApphostError::SandboxUnavailable);
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    return Err(ApphostError::SandboxUnavailable);

    let offset = u64::from_le_bytes(
        header[32..40]
            .try_into()
            .map_err(|_| ApphostError::SandboxUnavailable)?,
    );
    let entry_size = u64::from(u16::from_le_bytes([header[54], header[55]]));
    let count = u16::from_le_bytes([header[56], header[57]]);
    if count > MAX_PROGRAM_HEADERS
        || entry_size < 56
        || entry_size
            .checked_mul(u64::from(count))
            .is_none_or(|size| size > MAX_PROGRAM_HEADER_BYTES)
    {
        return Err(ApphostError::SandboxUnavailable);
    }
    for index in 0..count {
        let at = offset
            .checked_add(
                entry_size
                    .checked_mul(u64::from(index))
                    .ok_or(ApphostError::SandboxUnavailable)?,
            )
            .ok_or(ApphostError::SandboxUnavailable)?;
        file.seek(SeekFrom::Start(at))
            .map_err(|_| ApphostError::SandboxUnavailable)?;
        let mut kind = [0_u8; 4];
        file.read_exact(&mut kind)
            .map_err(|_| ApphostError::SandboxUnavailable)?;
        if u32::from_le_bytes(kind) == 3 {
            return Err(ApphostError::SandboxUnavailable);
        }
    }
    Ok(())
}

/// Small purpose-built syscall allowlist for a single-threaded static app.
/// Network, process creation, process inspection, kernel interfaces, and
/// resource-limit mutation are absent by construction.
fn syscall_allowlist() -> Vec<u32> {
    #[allow(clippy::useless_conversion)]
    let mut calls: Vec<u32> = vec![
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_close,
        libc::SYS_fstat,
        libc::SYS_lseek,
        libc::SYS_mmap,
        libc::SYS_mprotect,
        libc::SYS_munmap,
        libc::SYS_brk,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_pread64,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_sched_yield,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_dup,
        libc::SYS_nanosleep,
        libc::SYS_getpid,
        libc::SYS_exit,
        libc::SYS_fcntl,
        libc::SYS_fsync,
        libc::SYS_fdatasync,
        libc::SYS_getdents64,
        libc::SYS_getcwd,
        libc::SYS_chdir,
        libc::SYS_fchmod,
        libc::SYS_gettimeofday,
        libc::SYS_getrlimit,
        libc::SYS_getuid,
        libc::SYS_getgid,
        libc::SYS_geteuid,
        libc::SYS_clock_gettime,
        libc::SYS_exit_group,
        libc::SYS_openat,
        libc::SYS_newfstatat,
        libc::SYS_readlinkat,
        libc::SYS_faccessat,
        libc::SYS_getrandom,
        libc::SYS_execve,
        libc::SYS_set_tid_address,
        libc::SYS_set_robust_list,
        libc::SYS_futex,
        libc::SYS_statx,
        libc::SYS_rseq,
        libc::SYS_close_range,
    ]
    .iter()
    .map(|syscall| *syscall as u32)
    .collect();
    #[cfg(target_arch = "x86_64")]
    calls.extend([
        libc::SYS_arch_prctl as u32,
        libc::SYS_poll as u32,
        libc::SYS_access as u32,
        libc::SYS_rename as u32,
        libc::SYS_mkdir as u32,
        libc::SYS_unlink as u32,
        libc::SYS_readlink as u32,
    ]);
    #[cfg(target_arch = "aarch64")]
    calls.extend([
        libc::SYS_ppoll as u32,
        libc::SYS_renameat as u32,
        libc::SYS_mkdirat as u32,
        libc::SYS_unlinkat as u32,
    ]);
    calls.sort_unstable();
    calls.dedup();
    #[cfg(target_arch = "x86_64")]
    calls.push(libc::SYS_prlimit64 as u32);
    #[cfg(target_arch = "aarch64")]
    calls.push(libc::SYS_prlimit64 as u32);
    calls.sort_unstable();
    calls.dedup();
    calls
}

/// Permit `prlimit64` only for querying limits. Rust's static standard library
/// queries RLIMIT_STACK during startup; allowing the syscall without checking
/// its new-limit pointer would let the application raise its hard ceilings.
fn syscall_filter() -> Vec<u64> {
    use bux_seccomp::bpf::{
        BPF_ABS, BPF_JEQ, BPF_JMP, BPF_K, BPF_LD, BPF_RET, BPF_W, SECCOMP_ARCH_OFFSET,
        SECCOMP_NR_OFFSET, SECCOMP_RET_ALLOW, SECCOMP_RET_KILL_PROCESS, instruction,
    };

    let calls = syscall_allowlist();
    let prlimit = libc::SYS_prlimit64 as u32;
    let ordinary = calls.iter().filter(|&&call| call != prlimit).count();
    let mut program = Vec::with_capacity(ordinary + 10);
    program.push(instruction(
        BPF_LD | BPF_W | BPF_ABS,
        0,
        0,
        SECCOMP_ARCH_OFFSET,
    ));
    // Wrong architecture skips the syscall checks and reaches the kill action.
    program.push(instruction(
        BPF_JMP | BPF_JEQ | BPF_K,
        0,
        (ordinary + 6) as u8,
        bux_seccomp::arch::AUDIT_ARCH,
    ));
    program.push(instruction(
        BPF_LD | BPF_W | BPF_ABS,
        0,
        0,
        SECCOMP_NR_OFFSET,
    ));
    for (index, &call) in calls.iter().filter(|&&call| call != prlimit).enumerate() {
        let remaining = ordinary - index + 5;
        program.push(instruction(
            BPF_JMP | BPF_JEQ | BPF_K,
            remaining as u8,
            0,
            call,
        ));
    }
    // prlimit64 is special-cased before the default kill. Load arg 2, the
    // new_limit pointer, and require both halves to be zero (read-only query).
    program.push(instruction(BPF_JMP | BPF_JEQ | BPF_K, 0, 4, prlimit));
    program.push(instruction(BPF_LD | BPF_W | BPF_ABS, 0, 0, 32));
    program.push(instruction(BPF_JMP | BPF_JEQ | BPF_K, 0, 2, 0));
    program.push(instruction(BPF_LD | BPF_W | BPF_ABS, 0, 0, 36));
    program.push(instruction(BPF_JMP | BPF_JEQ | BPF_K, 1, 0, 0));
    program.push(instruction(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_KILL_PROCESS));
    program.push(instruction(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ALLOW));
    program
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_excludes_network_process_creation_and_inspection() {
        let calls = syscall_allowlist();
        for forbidden in [
            libc::SYS_clone,
            libc::SYS_fork,
            libc::SYS_vfork,
            libc::SYS_clone3,
        ] {
            assert!(!calls.contains(&(forbidden as u32)));
        }
        #[cfg(target_arch = "x86_64")]
        let denied = [
            41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 62, 101, 105,
            106, 113, 114, 117, 119, 155, 157, 160, 161, 165, 166, 250, 272, 298, 308, 310, 311,
            321, 424, 425, 426, 427, 434, 435, 438,
        ];
        #[cfg(target_arch = "aarch64")]
        let denied = [
            39, 40, 41, 51, 91, 97, 117, 129, 130, 131, 143, 144, 145, 146, 147, 149, 159, 164,
            167, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 219,
            220, 241, 242, 268, 270, 271, 280, 424, 425, 426, 427, 434, 435, 438,
        ];
        for denied in denied {
            assert!(!calls.contains(&(denied as u32)));
        }
        assert!(calls.contains(&(libc::SYS_write as u32)));
        assert!(calls.contains(&(libc::SYS_execve as u32)));
    }

    #[test]
    fn elf_header_parser_rejects_non_elf_and_missing_file() {
        assert!(ensure_static_elf(Path::new("/dev/null")).is_err());
        assert!(ensure_static_elf(Path::new("/i2pr/no-such-file")).is_err());
    }
}
