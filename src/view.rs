// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Skin-agnostic view models, incremental [`Delta`]s, [`apply`] and [`diff`].
//!
//! The data worker owns the normalized store and projects it into a
//! [`ViewModel`] for the current selection. It sends one
//! [`crate::DataEvent::Snapshot`] and then [`crate::DataEvent::Patch`]es
//! produced by [`diff`]; the UI keeps a mirror up to date with [`apply`].

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::dto::{AuthSessionDto, IngressRouteDto, IngressUpstreamDto, TlsHealthDto};
use crate::model::{RepoId, SubmoduleId};

/// Mask private LAN and documentation IPv4 addresses:
/// - RFC 1918 private IPv4 subnets (Class A, Class B, and Class C)
/// - RFC 5737: 192.0.2.0/24 (TEST-NET-1)
///
/// Masks octet 3 with `###` (e.g. `192.0.2.42` -> `192.0.###.42`).
fn mask_private_ipv4(s: &mut String) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut replacements = Vec::new();

    while i < len {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'/') {
            while i < len && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            continue;
        }

        let start = i;
        let mut octets = [0u32; 4];
        let mut oct_lens = [0usize; 4];
        let mut valid = true;

        for (oct_idx, oct_slot) in octets.iter_mut().enumerate() {
            let mut val = 0u32;
            let mut count = 0;
            while i < len && bytes[i].is_ascii_digit() && count < 3 {
                val = val * 10 + (bytes[i] - b'0') as u32;
                count += 1;
                i += 1;
            }
            if count == 0 || val > 255 {
                valid = false;
                break;
            }
            *oct_slot = val;
            oct_lens[oct_idx] = count;

            if oct_idx < 3 {
                if i < len && bytes[i] == b'.' {
                    i += 1;
                } else {
                    valid = false;
                    break;
                }
            }
        }

        if valid && (i == len || !bytes[i].is_ascii_alphanumeric()) {
            let o1 = octets[0];
            let o2 = octets[1];
            let is_match = (o1 == 10)
                || (o1 == 172 && (16..=31).contains(&o2))
                || (o1 == 192 && o2 == 168)
                || (o1 == 192 && o2 == 0);

            if is_match {
                let o3_start = start + oct_lens[0] + 1 + oct_lens[1] + 1;
                let o3_end = o3_start + oct_lens[2];
                replacements.push((o3_start, o3_end));
            }
        }
    }

    for (start, end) in replacements.into_iter().rev() {
        s.replace_range(start..end, "###");
    }
}

/// Sanitize a log message for public display:
/// - Partially masks private LAN and documentation IPv4 addresses (`<net>.<subnet>.<host>` -> `<net>.###.<host>`);
/// - Redacts internal domain suffixes (`.internal.local`, `.local`, `.internal`, `.lan`);
/// - Applies optional host mappings from `MU_REDACT_HOSTNAMES` (when running on native platforms);
/// - Truncates deep filesystem paths (`/a/b/c/seg1/seg2` -> `.../seg1/seg2`).
#[must_use]
pub fn sanitize_public_log(raw: &str) -> String {
    let mut s = String::from(raw);

    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(mappings) = std::env::var("MU_REDACT_HOSTNAMES") {
        for mapping in mappings.split(',') {
            if let Some((from, to)) = mapping.split_once('=') {
                s = s.replace(from.trim(), to.trim());
            } else if let Some((from, to)) = mapping.split_once(':') {
                s = s.replace(from.trim(), to.trim());
            }
        }
    }

    s = s
        .replace(".internal.local", "")
        .replace(".local", "")
        .replace(".internal", "")
        .replace(".lan", "");

    mask_private_ipv4(&mut s);

    // Truncate deep absolute paths with >= 3 leading segments to `.../seg1/seg2`.
    let mut out = String::with_capacity(s.len());
    for token in s.split_inclusive(' ') {
        let trimmed = token.trim_end_matches(' ');
        let trail = &token[trimmed.len()..];
        let (lead_punct, core_tok) = match trimmed.chars().next() {
            Some(c @ ('(' | '[' | '"' | '\'')) => {
                (&trimmed[..c.len_utf8()], &trimmed[c.len_utf8()..])
            }
            _ => ("", trimmed),
        };
        let (core_path, tail_punct) = match core_tok.chars().last() {
            Some(c @ (')' | ']' | '"' | '\'' | ',' | ';')) => {
                let idx = core_tok.len() - c.len_utf8();
                (&core_tok[..idx], &core_tok[idx..])
            }
            _ => (core_tok, ""),
        };
        if core_path.starts_with('/') && core_path.matches('/').count() >= 3 {
            let parts: Vec<&str> = core_path.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 3 {
                out.push_str(&format!(
                    "{lead_punct}.../{}/{}{tail_punct}{trail}",
                    parts[parts.len() - 2],
                    parts[parts.len() - 1]
                ));
                continue;
            }
        }
        out.push_str(token);
    }
    out
}

/// Default maximum number of sanitized build log lines retained per CI job.
pub const DEFAULT_MAX_JOB_LOG_LINES: usize = 2_000;

/// Strip ANSI CSI escape codes (`\x1b[...m`) from a raw build log, apply
/// [`sanitize_public_log`] to every non-empty line, and return the last
/// `max_lines` lines.
#[must_use]
pub fn sanitize_ansi_log_lines(raw: &str, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in raw.lines() {
        let mut stripped = String::with_capacity(raw_line.len());
        let mut chars = raw_line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                if chars.peek() == Some(&'[') {
                    let _ = chars.next();
                    while let Some(&c) = chars.peek() {
                        let _ = chars.next();
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
            } else if !ch.is_control() || ch == '\t' {
                stripped.push(ch);
            }
        }
        let trimmed = stripped.trim();
        if !trimmed.is_empty() {
            lines.push(sanitize_public_log(trimmed));
        }
    }
    if lines.len() > max_lines {
        let drop_n = lines.len() - max_lines;
        lines.drain(0..drop_n);
    }
    lines
}

/// Overall CI state of a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoStatus {
    #[default]
    Unknown,
    Passing,
    Running,
    Failing,
}

/// Repository visibility. Private repos only reach authenticated sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Public,
    #[serde(alias = "internal")]
    Private,
}

/// One submodule of a superproject, as pinned on its default branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmoduleRef {
    pub path: String,
    pub repo: RepoId,
    pub pinned_sha: String,
}

/// A superproject that includes this repository as a submodule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentRef {
    pub repo: RepoId,
    pub path: String,
}

/// A/B deployment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    #[default]
    A,
    B,
}

impl Slot {
    /// The other slot.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Outcome of a deployment's integration gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    /// Tests still running (or not started).
    #[default]
    Pending,
    Promoted,
    RolledBack,
    Tainted,
}

impl GateOutcome {
    /// Whether the gate reached a final verdict.
    #[must_use]
    pub fn is_final(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

/// Why a deployment was rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackReason {
    CriticalTestFailed,
    LeaseExpired,
    Manual,
}

/// CD half of a scorecard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdScore {
    pub deployment_id: String,
    pub active_slot: Slot,
    pub outcome: GateOutcome,
    pub it_passed: u32,
    pub it_total: u32,
}

/// Scorecard for one repository (CI + CD joined in the worker).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoScore {
    pub id: RepoId,
    pub visibility: Visibility,
    pub default_branch: String,
    pub status: RepoStatus,
    /// Quality score in percent (0..=100).
    pub score: u32,
    /// Line coverage in basis points (0..=10_000).
    pub coverage_bp: u32,
    /// Benchmark delta vs. the default branch, basis points (signed).
    pub bench_delta_bp: i32,
    pub latest_job_id: Option<u64>,
    pub submodules: Vec<SubmoduleRef>,
    pub parents: Vec<ParentRef>,
    pub cd: Option<CdScore>,
}

impl RepoScore {
    /// Scorecard with only an id (everything else unknown / empty).
    #[must_use]
    pub fn new(id: RepoId) -> Self {
        Self {
            id,
            visibility: Visibility::Public,
            default_branch: "main".into(),
            status: RepoStatus::Unknown,
            score: 0,
            coverage_bp: 0,
            bench_delta_bp: 0,
            latest_job_id: None,
            submodules: Vec::new(),
            parents: Vec::new(),
            cd: None,
        }
    }
}

/// A pipeline stage, drawn as a station column on the Trainway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Station {
    /// Stage id from the workflow definition (`clippy-check`).
    pub id: String,
    /// Short label (`CLIPPY`).
    pub label: String,
}

/// Block signal state of one stage for one train.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    #[default]
    #[serde(
        alias = "queued",
        alias = "Queued",
        alias = "Pending",
        alias = "PENDING"
    )]
    Pending,
    #[serde(alias = "Running", alias = "RUNNING", alias = "in_progress")]
    Running,
    #[serde(
        alias = "completed",
        alias = "Completed",
        alias = "Passed",
        alias = "PASSED"
    )]
    Passed,
    #[serde(
        alias = "Failed",
        alias = "FAILED",
        alias = "cancelled",
        alias = "Cancelled"
    )]
    Failed,
    #[serde(alias = "Skipped", alias = "SKIPPED")]
    Skipped,
}

impl StageState {
    /// Parse a stage state string (`"Completed"`, `"Passed"`, `"Failed"`, `"Running"`, `"Queued"`, `"Pending"`, `"Skipped"`).
    #[must_use]
    pub fn from_status_str(s: &str, exit_code: i32) -> Self {
        if s.eq_ignore_ascii_case("failed") || s.eq_ignore_ascii_case("cancelled") || exit_code != 0
        {
            Self::Failed
        } else if s.eq_ignore_ascii_case("completed") || s.eq_ignore_ascii_case("passed") {
            Self::Passed
        } else if s.eq_ignore_ascii_case("running") || s.eq_ignore_ascii_case("in_progress") {
            Self::Running
        } else if s.eq_ignore_ascii_case("skipped") {
            Self::Skipped
        } else {
            Self::Pending
        }
    }

    /// Short uppercase label for tables and logs.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
            Self::Skipped => "SKIPPED",
        }
    }
}

/// Pull-request state of a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    #[default]
    None,
    #[serde(alias = "draft")]
    Open,
    Merged,
    Closed,
}

/// Why a siding is fading out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FadeReason {
    Merged,
    Deleted,
}

/// Life cycle of a siding on the Trainway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SidingPhase {
    /// Branch just created: dashed track, `NEW SIDING UNDER CONSTRUCTION`.
    /// Ends when the first job on the branch is dequeued on a node.
    UnderConstruction,
    Active,
    /// Merged or deleted; the worker removes the siding 2 s after `since`.
    Fading {
        reason: FadeReason,
        since_epoch_ms: u64,
    },
}

/// A non-default branch drawn as a siding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    pub ref_name: String,
    pub base_ref: String,
    /// Station index where the siding leaves the main line.
    pub fork_stage: u32,
    pub pr_state: PrState,
    pub last_job: Option<u64>,
    pub phase: SidingPhase,
}

/// A CI job drawn as a train.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Train {
    pub job_id: u64,
    pub ref_name: String,
    /// Station index of the current stage.
    pub stage: u32,
    /// One signal per station (same order as [`RepoTopology::stations`]).
    pub stage_states: Vec<StageState>,
    /// Fabric node carrying the job; `None` while queued.
    pub node_id: Option<String>,
    /// The carrying node left the fabric (`NODE LOST`).
    pub node_lost: bool,
}

impl Train {
    /// A failed stage stops the train (reverse video + `STOP`).
    #[must_use]
    pub fn failed(&self) -> bool {
        self.stage_states.contains(&StageState::Failed)
    }

    /// Queued: no node yet.
    #[must_use]
    pub fn queued(&self) -> bool {
        self.node_id.is_none()
    }
}

/// Railway model of one repository (or the selected submodule's repo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoTopology {
    pub repo: RepoId,
    /// Set when the topology is shown for a submodule of the selection.
    pub submodule: Option<SubmoduleId>,
    pub default_branch: String,
    pub stations: Vec<Station>,
    pub branches: Vec<Branch>,
    pub trains: Vec<Train>,
    /// This repository's own submodules (interchange markers).
    pub submodules: Vec<SubmoduleRef>,
    /// Topology change counter shown as `FABRIC REV n`.
    pub fabric_rev: u64,
    /// The parent's gitlink points at an older commit than this repo's
    /// default branch head ("submodule bump pending" interchange).
    pub submodule_bump_pending: bool,
}

impl RepoTopology {
    /// `STOP` annunciator: any train has a failed stage.
    #[must_use]
    pub fn stop(&self) -> bool {
        self.trains.iter().any(Train::failed)
    }
}

/// Integration-test state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestState {
    #[default]
    Pending,
    Running,
    Passed,
    Failed,
}

/// One checkpoint on the integration gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateTest {
    pub name: String,
    /// `[C]` flag: a failure rolls the deployment back.
    pub critical: bool,
    pub state: TestState,
    pub duration_ms: u64,
    /// Crate the test belongs to (highlighted for the selected submodule).
    pub crate_name: Option<String>,
    pub log_excerpt: Option<String>,
}

/// Live integration gate of a deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentGate {
    pub deployment_id: String,
    pub repo: RepoId,
    pub service: String,
    pub artifact_digest: String,
    pub target_node: String,
    pub staged_slot: Slot,
    pub restore_slot: Slot,
    pub lease_expires_epoch_ms: u64,
    pub outcome: GateOutcome,
    pub rollback_reason: Option<RollbackReason>,
    pub tainted: bool,
    pub tests: Vec<GateTest>,
}

impl DeploymentGate {
    /// Passed test count.
    #[must_use]
    pub fn passed(&self) -> u32 {
        count_u32(self.tests.iter().filter(|t| t.state == TestState::Passed))
    }

    /// Total test count.
    #[must_use]
    pub fn total(&self) -> u32 {
        count_u32(self.tests.iter())
    }

    /// Every critical test passed.
    #[must_use]
    pub fn all_critical_passed(&self) -> bool {
        self.tests
            .iter()
            .filter(|t| t.critical)
            .all(|t| t.state == TestState::Passed)
    }

    /// Any critical test failed.
    #[must_use]
    pub fn any_critical_failed(&self) -> bool {
        self.tests
            .iter()
            .any(|t| t.critical && t.state == TestState::Failed)
    }

    /// `PROMOTE` junction lamp.
    #[must_use]
    pub fn promote_lit(&self) -> bool {
        self.outcome == GateOutcome::Promoted
            || (self.outcome == GateOutcome::Pending && self.all_critical_passed())
    }

    /// `ROLLBACK` junction lamp.
    #[must_use]
    pub fn rollback_lit(&self) -> bool {
        self.outcome == GateOutcome::RolledBack || self.any_critical_failed()
    }

    /// History-strip entry for this gate.
    #[must_use]
    pub fn summary(&self) -> DeploymentSummary {
        DeploymentSummary {
            deployment_id: self.deployment_id.clone(),
            artifact_digest: self.artifact_digest.clone(),
            it_passed: self.passed(),
            it_total: self.total(),
            outcome: self.outcome,
        }
    }
}

fn count_u32<I: Iterator>(it: I) -> u32 {
    u32::try_from(it.count()).unwrap_or(u32::MAX)
}

/// History-strip entry: `digest IT n/m`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentSummary {
    pub deployment_id: String,
    pub artifact_digest: String,
    pub it_passed: u32,
    pub it_total: u32,
    pub outcome: GateOutcome,
}

/// Active TTL-bounded deployment lease (`CD > Active Leases`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdLease {
    pub lease_id: String,
    pub deployment_id: String,
    pub target_node: String,
    pub service: String,
    pub artifact_digest: String,
    pub active_slot: Slot,
    pub candidate_slot: Slot,
    pub ttl_remaining_ms: u64,
    pub promoted: bool,
}

/// Fabric worker node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FabricNode {
    pub id: String,
    pub online: bool,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub active_slots: u32,
    #[serde(default)]
    pub max_slots: u32,
    /// CPU load in basis points (`320` = `3.20%`).
    #[serde(default)]
    pub cpu_load_bp: u32,
    #[serde(default)]
    pub ram_used_mb: u64,
    #[serde(default)]
    pub ram_total_mb: u64,
    #[serde(default)]
    pub btrfs_enabled: bool,
    #[serde(default)]
    pub exclusive_locked: bool,
    #[serde(default)]
    pub last_seen_secs_ago: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub toolchains: Vec<String>,
}

impl FabricNode {
    /// Construct a node with minimal `id` + `online` state (`max_slots = 1`).
    #[must_use]
    pub fn new(id: impl Into<String>, online: bool) -> Self {
        Self {
            id: id.into(),
            online,
            max_slots: 1,
            ..Self::default()
        }
    }
}

/// Declarative `MuIsolation` sandbox profile (`FLT > Sandbox Profiles`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxProfileSummary {
    pub profile_name: String,
    pub backend: String,
    pub max_pids: u32,
    pub max_slots: u32,
    pub thread_budget: u32,
    pub hardware_target: String,
}

/// Package ecosystem in `MuRegist` (`REG > Packages`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PackageEcosystem {
    #[default]
    #[serde(alias = "CARGO", alias = "Cargo")]
    Cargo,
    #[serde(alias = "nuget", alias = "NUGET", alias = "NuGet")]
    NuGet,
    #[serde(alias = "MAVEN", alias = "Maven")]
    Maven,
}

impl PackageEcosystem {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cargo => "CARGO",
            Self::NuGet => "NUGET",
            Self::Maven => "MAVEN",
        }
    }
}

/// One package in the unified `MuRegist` catalog (`REG > Packages`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPackage {
    pub ecosystem: PackageEcosystem,
    pub name: String,
    pub latest_version: String,
    pub versions_count: u32,
    pub sha256_prefix: String,
    pub deps_count: u32,
    pub yanked: bool,
    pub tainted: bool,
    pub taint_reason: Option<String>,
    pub versions: Vec<String>,
}

/// Canary bundle lifecycle state (`REG > Canary Bundles & Taints`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanaryBundleState {
    #[default]
    #[serde(alias = "CANDIDATE", alias = "pending", alias = "staged")]
    Candidate,
    #[serde(alias = "PROMOTED", alias = "active")]
    Promoted,
    #[serde(alias = "TAINTED")]
    Tainted,
    #[serde(alias = "ROLLED_BACK", alias = "ROLLED BACK")]
    RolledBack,
}

impl CanaryBundleState {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Candidate => "CANDIDATE",
            Self::Promoted => "PROMOTED",
            Self::Tainted => "TAINTED",
            Self::RolledBack => "ROLLED BACK",
        }
    }
}

/// Canary release bundle & taint provenance record (`REG > Canary Bundles & Taints`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanaryBundle {
    pub bundle_id: String,
    pub repo: RepoId,
    pub service: String,
    pub artifact_digest: String,
    pub state: CanaryBundleState,
    pub taint_reason: Option<String>,
    pub deployment_id: String,
    pub trace_id: String,
}

/// Content-Addressable Storage health & scrub telemetry (`REG > CAS Storage Health`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CasHealth {
    pub blob_count: u64,
    pub dedup_bytes: u64,
    pub raw_bytes: u64,
    /// Deduplication ratio in basis points (`245` = `2.45x`).
    pub dedup_ratio_bp: u32,
    pub gc_watermark_pct: u32,
    pub last_scrub_epoch_ms: u64,
    pub corrupt_blobs: u32,
}

/// Aggregate state for `Area::Registry` (`REG`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RegistryView {
    pub packages: Vec<RegistryPackage>,
    pub canaries: Vec<CanaryBundle>,
    pub cas: CasHealth,
}

/// Vertical layer in the 3-tier `Smartscape` topology (`OBS > Smartscape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartscapeLayer {
    #[default]
    Service,
    Process,
    Host,
}

impl SmartscapeLayer {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Service => "SERVICES",
            Self::Process => "PROCESSES",
            Self::Host => "HOSTS",
        }
    }
}

/// One node in the `Smartscape` topology (`OBS > Smartscape`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartscapeNode {
    pub id: String,
    pub label: String,
    pub layer: SmartscapeLayer,
    /// Request rate in hundredths of RPS (`1250` = `12.50 rps`).
    pub req_per_sec_bp: u32,
    /// Error rate in basis points (`150` = `1.50%`).
    pub error_rate_bp: u32,
    pub p95_ms: u32,
    pub healthy: bool,
}

/// Directed call edge in `Smartscape` (`OBS > Smartscape`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartscapeEdge {
    pub from_id: String,
    pub to_id: String,
    pub protocol: String,
    pub call_rate_rps: u32,
    pub error_rate_bp: u32,
    pub p95_ms: u32,
}

/// One W3C trace span in a `PurePath` waterfall (`OBS > PurePath`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSpan {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub operation: String,
    pub start_offset_us: u64,
    pub duration_us: u64,
    pub self_time_us: u64,
    pub critical_path: bool,
    pub error: bool,
    pub hardware_class: String,
}

/// Distributed trace summary + span tree (`OBS > PurePath`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurePathTrace {
    pub trace_id: String,
    pub root_service: String,
    pub root_operation: String,
    pub domain: String,
    pub duration_ms: u32,
    pub error: bool,
    pub slow_p99: bool,
    pub started_epoch_ms: u64,
    pub spans: Vec<TraceSpan>,
}

/// Causal SLO problem category (`OBS > Problems & SLOs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemCategory {
    #[default]
    #[serde(alias = "latency", alias = "benchmark_regression")]
    LatencyRegression,
    #[serde(alias = "error_rate", alias = "error_spike")]
    ErrorRateSpike,
    #[serde(alias = "flaky", alias = "test_flakiness")]
    FlakyTest,
    #[serde(alias = "canary", alias = "taint")]
    CanaryTaint,
}

impl ProblemCategory {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LatencyRegression => "LATENCY REGRESSION",
            Self::ErrorRateSpike => "ERROR RATE SPIKE",
            Self::FlakyTest => "FLAKY TEST",
            Self::CanaryTaint => "CANARY TAINT",
        }
    }
}

/// Causal anomaly / SLO breach card (`OBS > Problems & SLOs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SloProblem {
    pub problem_id: String,
    pub title: String,
    pub category: ProblemCategory,
    pub severity: String,
    /// SLO burn rate in basis points (`420` = `4.2x`).
    pub burn_rate_bp: u32,
    pub impacted_entity: String,
    pub root_cause_trace_id: String,
    pub active: bool,
}

/// Log severity level (`OBS > Logs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSeverity {
    #[default]
    #[serde(
        alias = "INFO",
        alias = "DEBUG",
        alias = "TRACE",
        alias = "debug",
        alias = "trace"
    )]
    Info,
    #[serde(alias = "WARN", alias = "WARNING", alias = "warning")]
    Warn,
    #[serde(alias = "ERROR", alias = "FATAL", alias = "fatal")]
    Error,
}

impl LogSeverity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// Sanitized OpenTelemetry log record (`OBS > Logs` & `PurePath` correlated log pane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtelLogEntry {
    pub timestamp_ms: u64,
    pub severity: LogSeverity,
    pub service: String,
    pub span_id: Option<String>,
    pub trace_id: String,
    pub message: String,
}

/// Golden-signal time-series & hardware class benchmark summary (`OBS > Metrics`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareMetricSeries {
    pub service: String,
    pub hardware_class: String,
    pub rps: u32,
    pub error_rate_bp: u32,
    pub p50_ms: u32,
    pub p95_ms: u32,
    pub p99_ms: u32,
    pub exemplar_trace_id: String,
    pub samples: Vec<u32>,
}

/// Read-only DuckDB hot buffer + Parquet cold tier health (`OBS > Data Store`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DataStoreHealth {
    pub hot_duckdb_rows: u64,
    pub hot_duckdb_bytes: u64,
    pub cold_parquet_partitions: u32,
    pub cold_parquet_rows: u64,
    pub cold_parquet_bytes: u64,
    /// Zstd compression ratio in basis points (`385` = `3.85x`).
    pub zstd_compression_bp: u32,
    pub retention_days: u32,
    pub oldest_watermark_iso: String,
}

/// Aggregate state for `Area::Observe` (`OBS`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ObserveView {
    pub smartscape_nodes: Vec<SmartscapeNode>,
    pub smartscape_edges: Vec<SmartscapeEdge>,
    pub traces: Vec<PurePathTrace>,
    pub problems: Vec<SloProblem>,
    pub logs: Vec<OtelLogEntry>,
    pub metrics: Vec<HardwareMetricSeries>,
    pub store: DataStoreHealth,
    #[serde(default)]
    pub ingress_routes: Vec<IngressRouteDto>,
    #[serde(default)]
    pub ingress_upstreams: Vec<IngressUpstreamDto>,
    #[serde(default)]
    pub tls_health: Option<TlsHealthDto>,
}

/// Current repository / submodule selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub repo: RepoId,
    pub submodule: Option<SubmoduleId>,
}

/// Where the view model comes from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DataSource {
    #[default]
    Live,
    /// Fixture scenario by name (`?scenario=`).
    Scenario(String),
    /// Restored from the CacheStorage offline snapshot.
    OfflineCache,
}

/// Alert counts per area (LCD badges next to the 5 area lamps on `C1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct AreaBadges {
    pub ci: u32,
    pub cd: u32,
    #[serde(default)]
    pub registry: u32,
    #[serde(default)]
    pub observe: u32,
    pub fleet: u32,
}

/// Operator authentication state surfaced in the top bar and used to gate
/// raw execution logs and job restart controls.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    #[default]
    Anonymous,
    InviteRequired {
        github_login: String,
    },
    Operator {
        github_login: String,
        role: Option<String>,
    },
}

impl AuthState {
    /// Derive [`AuthState`] from an [`AuthSessionDto`] response.
    #[must_use]
    pub fn from_session(dto: &AuthSessionDto) -> Self {
        if dto.authenticated || dto.invite_bound {
            Self::Operator {
                github_login: dto
                    .github_login
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "operator".into()),
                role: dto.role.clone(),
            }
        } else if let Some(login) = dto.github_login.as_ref().filter(|s| !s.is_empty()) {
            Self::InviteRequired {
                github_login: login.clone(),
            }
        } else {
            Self::Anonymous
        }
    }

    /// Whether the session is an authenticated, invite-bound operator.
    #[must_use]
    pub fn is_operator(&self) -> bool {
        matches!(self, Self::Operator { .. })
    }

    /// Short top-bar badge label (`READ-ONLY`, `INVITE REQ (@login)`, `@login`).
    #[must_use]
    pub fn badge_label(&self) -> String {
        match self {
            Self::Anonymous => "READ-ONLY".into(),
            Self::InviteRequired { github_login } => format!("INVITE REQ (@{github_login})"),
            Self::Operator { github_login, .. } => format!("@{github_login}"),
        }
    }
}

/// One pipeline stage execution record within a [`CiJobSummary`] (`CI > Queue & Logs` and `FLT > Worker Nodes`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CiJobStage {
    pub id: String,
    pub name: String,
    pub assigned_node: Option<String>,
    pub threads: u32,
    pub state: StageState,
    pub duration_secs: u64,
    pub exit_code: i32,
}

/// Queue or recent CI job record with full string `job_id`, per-stage breakdown, and raw build log lines.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CiJobSummary {
    /// Canonical string job ID on `ActionRunner-controller` (e.g. `"job-1794054497-8c395767"`).
    pub job_id_str: String,
    /// Packed `u64` job ID matching [`Train::job_id`].
    pub job_id: u64,
    pub repo: RepoId,
    pub ref_name: String,
    pub commit_sha: String,
    /// `"active"`, `"queued"`, `"passed"`, `"failed"`.
    pub queue_state: String,
    pub elapsed_secs: u64,
    pub wait_secs: u64,
    pub quality_score: u32,
    pub coverage_bp: u32,
    pub tests_passed: u32,
    pub tests_total: u32,
    pub benchmark_summary: String,
    pub stages: Vec<CiJobStage>,
    /// Sanitized raw log lines fetched from `GET /api/jobs/{id}/log` (empty until loaded).
    #[serde(default)]
    pub raw_log_lines: Vec<String>,
}

/// Full view model snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ViewModel {
    pub source: DataSource,
    /// Scorecards, sorted by [`RepoId`].
    pub repos: Vec<RepoScore>,
    pub selection: Option<Selection>,
    /// Trainway of the selection (submodule repo when one is selected).
    pub topology: Option<RepoTopology>,
    /// Active (or most recent) deployment gate of the selected parent repo.
    pub gate: Option<DeploymentGate>,
    /// History strip, newest first.
    pub history: Vec<DeploymentSummary>,
    /// Active TTL-bounded deployment leases.
    #[serde(default)]
    pub leases: Vec<CdLease>,
    /// Fabric nodes, sorted by id.
    pub nodes: Vec<FabricNode>,
    /// Declarative `MuIsolation` sandbox profiles.
    #[serde(default)]
    pub sandboxes: Vec<SandboxProfileSummary>,
    /// `Area::Registry` (`REG`) view state.
    #[serde(default)]
    pub registry: RegistryView,
    /// `Area::Observe` (`OBS`) view state.
    #[serde(default)]
    pub observe: ObserveView,
    /// Set when the data is stale (offline snapshot): when it was saved.
    pub stale_since_epoch_ms: Option<u64>,
    /// Operator authentication state.
    #[serde(default)]
    pub auth: AuthState,
    /// Active, queued, and recent CI job records (`CI > Queue & Logs` and `FLT > Worker Nodes`).
    #[serde(default)]
    pub ci_jobs: Vec<CiJobSummary>,
}

/// Incremental change to the view model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Delta {
    Source(DataSource),
    RepoUpsert(RepoScore),
    RepoRemoved(RepoId),
    Selection(Option<Selection>),
    /// Replace the whole topology (selection or station set changed).
    Topology(Option<RepoTopology>),
    FabricRev(u64),
    SubmoduleBumpPending(bool),
    BranchUpsert(Branch),
    BranchRemoved(String),
    TrainUpsert(Train),
    TrainRemoved(u64),
    /// Replace the whole gate (deployment or test list changed).
    Gate(Option<DeploymentGate>),
    GateTestUpsert(GateTest),
    GateStatus {
        outcome: GateOutcome,
        rollback_reason: Option<RollbackReason>,
        tainted: bool,
        lease_expires_epoch_ms: u64,
    },
    History(Vec<DeploymentSummary>),
    Leases(Vec<CdLease>),
    NodeUpsert(FabricNode),
    NodeRemoved(String),
    Sandboxes(Vec<SandboxProfileSummary>),
    Registry(RegistryView),
    Observe(ObserveView),
    Stale(Option<u64>),
    Auth(AuthState),
    CiJobs(Vec<CiJobSummary>),
}

fn upsert_sorted<T, K: Ord + ?Sized>(v: &mut Vec<T>, item: T, key: impl Fn(&T) -> &K) {
    match v.binary_search_by(|x| key(x).cmp(key(&item))) {
        Ok(i) => v[i] = item,
        Err(i) => v.insert(i, item),
    }
}

fn remove_sorted<T, K: Ord + ?Sized>(v: &mut Vec<T>, k: &K, key: impl Fn(&T) -> &K) {
    if let Ok(i) = v.binary_search_by(|x| key(x).cmp(k)) {
        v.remove(i);
    }
}

/// Apply one delta. Deltas that target an absent topology / gate are
/// ignored (the next snapshot or replace delta carries them).
pub fn apply(vm: &mut ViewModel, delta: &Delta) {
    match delta {
        Delta::Source(s) => vm.source = s.clone(),
        Delta::RepoUpsert(r) => upsert_sorted(&mut vm.repos, r.clone(), |x| &x.id),
        Delta::RepoRemoved(id) => remove_sorted(&mut vm.repos, id, |x| &x.id),
        Delta::Selection(s) => vm.selection = s.clone(),
        Delta::Topology(t) => vm.topology = t.clone(),
        Delta::Gate(g) => vm.gate = g.clone(),
        Delta::History(h) => vm.history = h.clone(),
        Delta::Leases(l) => vm.leases = l.clone(),
        Delta::NodeUpsert(n) => upsert_sorted(&mut vm.nodes, n.clone(), |x| x.id.as_str()),
        Delta::NodeRemoved(id) => remove_sorted(&mut vm.nodes, id.as_str(), |x| x.id.as_str()),
        Delta::Sandboxes(s) => vm.sandboxes = s.clone(),
        Delta::Registry(r) => vm.registry = r.clone(),
        Delta::Observe(o) => vm.observe = o.clone(),
        Delta::Stale(s) => vm.stale_since_epoch_ms = *s,
        Delta::Auth(a) => vm.auth = a.clone(),
        Delta::CiJobs(j) => vm.ci_jobs = j.clone(),
        Delta::FabricRev(_)
        | Delta::SubmoduleBumpPending(_)
        | Delta::BranchUpsert(_)
        | Delta::BranchRemoved(_)
        | Delta::TrainUpsert(_)
        | Delta::TrainRemoved(_) => {
            if let Some(t) = vm.topology.as_mut() {
                apply_topology(t, delta);
            }
        }
        Delta::GateTestUpsert(_) | Delta::GateStatus { .. } => {
            if let Some(g) = vm.gate.as_mut() {
                apply_gate(g, delta);
            }
        }
    }
}

fn apply_topology(t: &mut RepoTopology, delta: &Delta) {
    match delta {
        Delta::FabricRev(r) => t.fabric_rev = *r,
        Delta::SubmoduleBumpPending(b) => t.submodule_bump_pending = *b,
        Delta::BranchUpsert(b) => {
            upsert_sorted(&mut t.branches, b.clone(), |x| x.ref_name.as_str());
        }
        Delta::BranchRemoved(r) => {
            remove_sorted(&mut t.branches, r.as_str(), |x| x.ref_name.as_str());
        }
        Delta::TrainUpsert(tr) => upsert_sorted(&mut t.trains, tr.clone(), |x| &x.job_id),
        Delta::TrainRemoved(id) => remove_sorted(&mut t.trains, id, |x| &x.job_id),
        _ => {}
    }
}

fn apply_gate(g: &mut DeploymentGate, delta: &Delta) {
    match delta {
        Delta::GateTestUpsert(t) => {
            if let Some(slot) = g.tests.iter_mut().find(|x| x.name == t.name) {
                *slot = t.clone();
            } else {
                g.tests.push(t.clone());
            }
        }
        Delta::GateStatus {
            outcome,
            rollback_reason,
            tainted,
            lease_expires_epoch_ms,
        } => {
            g.outcome = *outcome;
            g.rollback_reason = *rollback_reason;
            g.tainted = *tainted;
            g.lease_expires_epoch_ms = *lease_expires_epoch_ms;
        }
        _ => {}
    }
}

/// Apply a patch in order.
pub fn apply_all(vm: &mut ViewModel, deltas: &[Delta]) {
    for d in deltas {
        apply(vm, d);
    }
}

/// Merge-walk two sorted lists, emitting upserts and removals.
fn diff_sorted<T: PartialEq + Clone, K: Ord + ?Sized>(
    old: &[T],
    new: &[T],
    key: impl Fn(&T) -> &K,
    upsert: impl Fn(T) -> Delta,
    remove: impl Fn(&T) -> Delta,
    out: &mut Vec<Delta>,
) {
    let (mut i, mut j): (usize, usize) = (0, 0);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some(a), Some(b)) => match key(a).cmp(key(b)) {
                core::cmp::Ordering::Equal => {
                    if a != b {
                        out.push(upsert(b.clone()));
                    }
                    i += 1;
                    j += 1;
                }
                core::cmp::Ordering::Less => {
                    out.push(remove(a));
                    i += 1;
                }
                core::cmp::Ordering::Greater => {
                    out.push(upsert(b.clone()));
                    j += 1;
                }
            },
            (Some(a), None) => {
                out.push(remove(a));
                i += 1;
            }
            (None, Some(b)) => {
                out.push(upsert(b.clone()));
                j += 1;
            }
            (None, None) => break,
        }
    }
}

/// Smallest practical delta list turning `old` into `new` (both canonical).
#[must_use]
pub fn diff(old: &ViewModel, new: &ViewModel) -> Vec<Delta> {
    let mut out = Vec::new();
    if old.source != new.source {
        out.push(Delta::Source(new.source.clone()));
    }
    diff_sorted(
        &old.repos,
        &new.repos,
        |x| &x.id,
        Delta::RepoUpsert,
        |x| Delta::RepoRemoved(x.id.clone()),
        &mut out,
    );
    if old.selection != new.selection {
        out.push(Delta::Selection(new.selection.clone()));
    }
    diff_topology(old.topology.as_ref(), new.topology.as_ref(), &mut out);
    diff_gate(old.gate.as_ref(), new.gate.as_ref(), &mut out);
    if old.history != new.history {
        out.push(Delta::History(new.history.clone()));
    }
    if old.leases != new.leases {
        out.push(Delta::Leases(new.leases.clone()));
    }
    diff_sorted(
        &old.nodes,
        &new.nodes,
        |x| x.id.as_str(),
        Delta::NodeUpsert,
        |x| Delta::NodeRemoved(x.id.clone()),
        &mut out,
    );
    if old.sandboxes != new.sandboxes {
        out.push(Delta::Sandboxes(new.sandboxes.clone()));
    }
    if old.registry != new.registry {
        out.push(Delta::Registry(new.registry.clone()));
    }
    if old.observe != new.observe {
        out.push(Delta::Observe(new.observe.clone()));
    }
    if old.stale_since_epoch_ms != new.stale_since_epoch_ms {
        out.push(Delta::Stale(new.stale_since_epoch_ms));
    }
    if old.auth != new.auth {
        out.push(Delta::Auth(new.auth.clone()));
    }
    if old.ci_jobs != new.ci_jobs {
        out.push(Delta::CiJobs(new.ci_jobs.clone()));
    }
    out
}

fn diff_topology(old: Option<&RepoTopology>, new: Option<&RepoTopology>, out: &mut Vec<Delta>) {
    match (old, new) {
        (None, None) => {}
        (Some(a), Some(b))
            if a.repo == b.repo
                && a.submodule == b.submodule
                && a.default_branch == b.default_branch
                && a.stations == b.stations
                && a.submodules == b.submodules =>
        {
            if a.fabric_rev != b.fabric_rev {
                out.push(Delta::FabricRev(b.fabric_rev));
            }
            if a.submodule_bump_pending != b.submodule_bump_pending {
                out.push(Delta::SubmoduleBumpPending(b.submodule_bump_pending));
            }
            diff_sorted(
                &a.branches,
                &b.branches,
                |x| x.ref_name.as_str(),
                Delta::BranchUpsert,
                |x| Delta::BranchRemoved(x.ref_name.clone()),
                out,
            );
            diff_sorted(
                &a.trains,
                &b.trains,
                |x| &x.job_id,
                Delta::TrainUpsert,
                |x| Delta::TrainRemoved(x.job_id),
                out,
            );
        }
        (_, b) => out.push(Delta::Topology(b.cloned())),
    }
}

fn diff_gate(old: Option<&DeploymentGate>, new: Option<&DeploymentGate>, out: &mut Vec<Delta>) {
    match (old, new) {
        (None, None) => {}
        (Some(a), Some(b))
            if a.deployment_id == b.deployment_id
                && a.repo == b.repo
                && a.service == b.service
                && a.artifact_digest == b.artifact_digest
                && a.target_node == b.target_node
                && a.staged_slot == b.staged_slot
                && a.restore_slot == b.restore_slot
                && b.tests.len() >= a.tests.len()
                && a.tests.iter().zip(&b.tests).all(|(x, y)| x.name == y.name) =>
        {
            for (i, t) in b.tests.iter().enumerate() {
                if a.tests.get(i) != Some(t) {
                    out.push(Delta::GateTestUpsert(t.clone()));
                }
            }
            if a.outcome != b.outcome
                || a.rollback_reason != b.rollback_reason
                || a.tainted != b.tainted
                || a.lease_expires_epoch_ms != b.lease_expires_epoch_ms
            {
                out.push(Delta::GateStatus {
                    outcome: b.outcome,
                    rollback_reason: b.rollback_reason,
                    tainted: b.tainted,
                    lease_expires_epoch_ms: b.lease_expires_epoch_ms,
                });
            }
        }
        (_, b) => out.push(Delta::Gate(b.cloned())),
    }
}
