// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! JSON DTOs of the read-only REST + SSE API the data worker consumes.
//!
//! This module is the client side of the contract documented in
//! `docs/api-contract.md`. ActionRunner-controller serves `/api/repos*` and
//! the CI event kinds on `/api/events`; MuConDe-controller serves
//! `/api/cd/*`; MuRegist serves `/api/reg/v1/*`; MuObserve serves
//! `/api/obs/v1/*`. All field names are `snake_case`, enums are `snake_case`
//! strings, timestamps are Unix epoch milliseconds and unknown fields are
//! ignored, so servers may add fields without breaking the console.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, Serialize};

pub use crate::view::{
    CanaryBundleState, GateOutcome, LogSeverity, PackageEcosystem, PrState, ProblemCategory,
    RepoStatus, RollbackReason, Slot, SmartscapeLayer, StageState, TestState, Visibility,
};

fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Convert an `ActionRunner-controller` job identifier (e.g.
/// `"job-1793563200-44444444"` or `"1234"`) into a deterministic `u64` that
/// preserves chronological ordering across jobs on the same branch.
#[must_use]
pub fn parse_job_id_str(s: &str) -> u64 {
    if let Ok(n) = s.parse::<u64>() {
        return n;
    }
    if let Some(rest) = s.strip_prefix("job-") {
        let mut parts = rest.split('-');
        let ts = parts
            .next()
            .and_then(|p| p.parse::<u64>().ok())
            .unwrap_or(0);
        let suffix = parts.next().unwrap_or("");
        let hex_prefix: String = suffix
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(6)
            .collect();
        let low = u64::from_str_radix(&hex_prefix, 16).unwrap_or(0);
        if ts > 0 || low > 0 {
            return (ts << 24) | (low & 0x00FF_FFFF);
        }
    }
    // Fallback deterministic FNV-1a 64-bit hash.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Format a `u64` job identifier for display.
///
/// Small integer job identifiers (e.g. `100`, `1234` in fixture scenarios)
/// format as decimal strings, whereas packed `job-{ts}-{sha}` identifiers
/// produced by [`parse_job_id_str`] format as their 6-character hexadecimal
/// commit SHA prefix (e.g. `"97ed67"`).
#[must_use]
pub fn format_job_id(job_id: u64) -> String {
    if (job_id >> 24) >= 1_000_000_000 {
        format!("{:06x}", job_id & 0x00FF_FFFF)
    } else {
        format!("{job_id}")
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawJobId {
    Num(u64),
    Str(String),
    Brief {
        #[serde(deserialize_with = "de_job_id")]
        job_id: u64,
    },
}

impl RawJobId {
    fn into_u64(self) -> u64 {
        match self {
            Self::Num(n) => n,
            Self::Str(s) => parse_job_id_str(&s),
            Self::Brief { job_id } => job_id,
        }
    }
}

fn de_job_id<'de, D>(d: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(RawJobId::deserialize(d)?.into_u64())
}

fn de_opt_job_id<'de, D>(d: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<RawJobId>::deserialize(d)?.map(RawJobId::into_u64))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawStageSignal {
    State(StageState),
    Signal { state: StageState },
}

fn de_stage_states<'de, D>(d: D) -> Result<Vec<StageState>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<Vec<RawStageSignal>>::deserialize(d)?.unwrap_or_default();
    Ok(raw
        .into_iter()
        .map(|s| match s {
            RawStageSignal::State(st) | RawStageSignal::Signal { state: st } => st,
        })
        .collect())
}

/// `submodules[]` entry of [`RepoDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmoduleDto {
    pub path: String,
    pub owner: String,
    pub repo: String,
    #[serde(default, deserialize_with = "null_default")]
    pub pinned_sha: String,
}

/// `parents[]` entry of [`RepoDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentDto {
    pub owner: String,
    pub repo: String,
    pub path: String,
}

/// `GET /api/repos` -> `[RepoDto]` or `{ "fabric_rev", "authenticated", "repos": [RepoDto] }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoDto {
    pub owner: String,
    pub repo: String,
    #[serde(default, deserialize_with = "null_default")]
    pub visibility: Visibility,
    #[serde(default, deserialize_with = "null_default")]
    pub default_branch: String,
    #[serde(default, deserialize_with = "null_default")]
    pub commit_sha: String,
    #[serde(default, deserialize_with = "de_opt_job_id")]
    pub latest_job_id: Option<u64>,
    /// Quality score, percent 0..=100.
    #[serde(default, deserialize_with = "null_default")]
    pub score: u32,
    /// Line coverage, percent 0.0..=100.0.
    #[serde(default, deserialize_with = "null_default")]
    pub coverage: f64,
    /// Benchmark delta vs. default branch, signed percent.
    #[serde(default, deserialize_with = "null_default")]
    pub bench_delta: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub status: RepoStatus,
    #[serde(default, deserialize_with = "null_default")]
    pub submodules: Vec<SubmoduleDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub parents: Vec<ParentDto>,
}

/// Envelope returned by `ActionRunner-controller` on `GET /api/repos`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoListResponseDto {
    #[serde(default)]
    pub fabric_rev: u64,
    #[serde(default)]
    pub authenticated: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub repos: Vec<RepoDto>,
}

/// `history[]` entry of [`RepoSummaryDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryHistoryDto {
    #[serde(default, deserialize_with = "de_job_id")]
    pub job_id: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub commit_sha: String,
    #[serde(default, deserialize_with = "null_default")]
    pub score: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub coverage: f64,
}

/// `GET /api/repos/{owner}/{repo}/summary`: the CI half of the scorecard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoSummaryDto {
    pub owner: String,
    pub repo: String,
    #[serde(default, deserialize_with = "null_default")]
    pub status: RepoStatus,
    #[serde(default, deserialize_with = "null_default")]
    pub commit_sha: String,
    #[serde(default, deserialize_with = "null_default")]
    pub score: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub coverage: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub bench_delta: f64,
    #[serde(default, deserialize_with = "de_opt_job_id")]
    pub latest_job_id: Option<u64>,
    /// Recent quality scores, oldest first (sparkline).
    #[serde(default, deserialize_with = "null_default")]
    pub recent_scores: Vec<u32>,
    /// Structured sparkline history returned by `ActionRunner-controller`.
    #[serde(default, deserialize_with = "null_default")]
    pub history: Vec<SummaryHistoryDto>,
    /// Submodules returned by `/api/repos/{owner}/{repo}/summary`.
    #[serde(default, deserialize_with = "null_default")]
    pub submodules: Vec<SubmoduleDto>,
    /// Parent superprojects returned by `/api/repos/{owner}/{repo}/summary`.
    #[serde(default, deserialize_with = "null_default")]
    pub parents: Vec<ParentDto>,
}

/// `stations[]` entry of [`TopologyDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationDto {
    pub id: String,
    #[serde(alias = "name")]
    pub label: String,
}

/// `branches[]` entry of [`TopologyDto`] (non-default refs only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchDto {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub base_ref: String,
    /// Station id where the branch was cut.
    #[serde(default, deserialize_with = "null_default")]
    pub fork_stage: String,
    #[serde(default, deserialize_with = "null_default")]
    pub pr_state: PrState,
    #[serde(default, deserialize_with = "de_opt_job_id")]
    pub last_job: Option<u64>,
}

/// `trains[]` entry of [`TopologyDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainDto {
    #[serde(deserialize_with = "de_job_id")]
    pub job_id: u64,
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub commit_sha: String,
    /// Station id of the current stage.
    #[serde(default, deserialize_with = "null_default")]
    pub stage: String,
    /// One state per station, in `stations[]` order.
    #[serde(default, deserialize_with = "de_stage_states")]
    pub stage_states: Vec<StageState>,
    #[serde(default)]
    pub node_id: Option<String>,
}

/// `GET /api/repos/{owner}/{repo}/topology`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyDto {
    pub owner: String,
    pub repo: String,
    pub default_branch: String,
    /// Head commit of the default branch (submodule-bump detection).
    #[serde(default, deserialize_with = "null_default")]
    pub head_sha: String,
    pub stations: Vec<StationDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub branches: Vec<BranchDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub trains: Vec<TrainDto>,
    #[serde(default)]
    pub fabric_rev: u64,
}

/// SSE `event: branch_created` on `/api/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchCreatedDto {
    pub owner: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub base_ref: String,
    #[serde(default, deserialize_with = "null_default")]
    pub fork_stage: String,
    #[serde(default)]
    pub fabric_rev: u64,
}

/// SSE `event: branch_deleted` on `/api/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchDeletedDto {
    pub owner: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(default)]
    pub fabric_rev: u64,
}

/// SSE `event: pr_state_changed` on `/api/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrStateChangedDto {
    pub owner: String,
    pub repo: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub pr_state: PrState,
    #[serde(default)]
    pub pr_number: Option<u64>,
    #[serde(default)]
    pub fabric_rev: u64,
}

/// SSE `event: job_stage_changed` on `/api/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobStageChangedDto {
    pub owner: String,
    pub repo: String,
    #[serde(deserialize_with = "de_job_id")]
    pub job_id: u64,
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// Station id of the stage that changed.
    #[serde(default, deserialize_with = "null_default")]
    pub stage: String,
    pub state: StageState,
    /// Optional full signal row (one per station); wins over `state`.
    #[serde(default, deserialize_with = "de_stage_states")]
    pub stage_states: Vec<StageState>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub fabric_rev: u64,
}

#[derive(Deserialize)]
struct RawNodeMembership {
    node_id: String,
    #[serde(default)]
    online: Option<bool>,
    #[serde(default)]
    change: Option<String>,
    #[serde(default)]
    fabric_rev: u64,
}

/// SSE `event: node_membership_changed` on `/api/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawNodeMembership")]
pub struct NodeMembershipChangedDto {
    pub node_id: String,
    pub online: bool,
    #[serde(default)]
    pub fabric_rev: u64,
}

impl From<RawNodeMembership> for NodeMembershipChangedDto {
    fn from(raw: RawNodeMembership) -> Self {
        let online = raw
            .online
            .unwrap_or(matches!(raw.change.as_deref(), Some("joined" | "online")));
        Self {
            node_id: raw.node_id,
            online,
            fabric_rev: raw.fabric_rev,
        }
    }
}

/// `GET /api/cd/deployments?repo={owner}/{repo}[&limit=N]` -> `[DeploymentDto]`,
/// newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentDto {
    pub deployment_id: String,
    pub owner: String,
    pub repo: String,
    pub service: String,
    pub artifact_digest: String,
    pub target_node: String,
    /// Slot currently serving traffic on the target.
    pub active_slot: Slot,
    /// Slot the candidate was staged into.
    pub staged_slot: Slot,
    pub outcome: GateOutcome,
    #[serde(default)]
    pub it_passed: u32,
    #[serde(default)]
    pub it_total: u32,
    #[serde(default)]
    pub started_epoch_ms: u64,
}

/// `tests[]` entry of [`GateDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateTestDto {
    pub name: String,
    pub critical: bool,
    pub state: TestState,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub crate_name: Option<String>,
    #[serde(default)]
    pub log_excerpt: Option<String>,
}

/// `GET /api/cd/deployments/{id}/gate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateDto {
    pub deployment_id: String,
    pub owner: String,
    pub repo: String,
    pub service: String,
    pub artifact_digest: String,
    pub target_node: String,
    pub staged_slot: Slot,
    pub restore_slot: Slot,
    #[serde(default)]
    pub lease_expires_epoch_ms: u64,
    pub outcome: GateOutcome,
    #[serde(default)]
    pub rollback_reason: Option<RollbackReason>,
    #[serde(default)]
    pub tainted: bool,
    /// Contract order (`.muconde/MuConDe.yml` -> `integration_tests`).
    pub tests: Vec<GateTestDto>,
}

/// `GET /api/cd/leases` -> `[LeaseDto]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseDto {
    pub lease_id: String,
    pub deployment_id: String,
    pub target_node: String,
    pub service: String,
    pub artifact_digest: String,
    pub active_slot: Slot,
    pub candidate_slot: Slot,
    #[serde(default)]
    pub expires_epoch_ms: u64,
    #[serde(default)]
    pub ttl_remaining_ms: u64,
    #[serde(default)]
    pub promoted: bool,
}

/// SSE `event: it_progress` on `/api/cd/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItProgressDto {
    pub deployment_id: String,
    pub test_name: String,
    pub critical: bool,
    pub state: TestState,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub timestamp_epoch_ms: u64,
}

/// SSE `event: promoted` on `/api/cd/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotedDto {
    pub deployment_id: String,
    /// Slot now serving traffic.
    pub slot: Slot,
}

/// SSE `event: rolled_back` on `/api/cd/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolledBackDto {
    pub deployment_id: String,
    pub restore_slot: Slot,
    pub reason: RollbackReason,
}

/// SSE `event: tainted` on `/api/cd/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaintedDto {
    pub deployment_id: String,
    pub artifact_digest: String,
    #[serde(default)]
    pub reason: String,
}

/// SSE `event: lease_expired` on `/api/cd/events`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseExpiredDto {
    pub deployment_id: String,
}

// ---------------------------------------------------------------- Registry DTOs (MuRegist)

/// `GET /api/reg/v1/packages` (`MuRegist` package entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDto {
    #[serde(default, deserialize_with = "null_default")]
    pub ecosystem: PackageEcosystem,
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub latest_version: String,
    #[serde(default)]
    pub versions_count: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub sha256_prefix: String,
    #[serde(default, deserialize_with = "null_default")]
    pub sha256: String,
    #[serde(default, alias = "dependency_count")]
    pub deps_count: u32,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub tainted: bool,
    #[serde(default)]
    pub taint_reason: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub versions: Vec<String>,
}

impl PackageDto {
    /// Return the 8-character SHA-256 prefix, falling back to `sha256` when `sha256_prefix` is empty.
    #[must_use]
    pub fn effective_sha256_prefix(&self) -> String {
        if !self.sha256_prefix.is_empty() {
            self.sha256_prefix.clone()
        } else {
            self.sha256.chars().take(8).collect()
        }
    }
}

/// `GET /api/reg/v1/canaries` (`MuRegist` canary bundle & taint entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanaryBundleDto {
    pub bundle_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub owner: String,
    #[serde(default, deserialize_with = "null_default")]
    pub repo: String,
    #[serde(default, alias = "service_name", deserialize_with = "null_default")]
    pub service: String,
    #[serde(default, alias = "binary_digest", deserialize_with = "null_default")]
    pub artifact_digest: String,
    #[serde(default, alias = "status", deserialize_with = "null_default")]
    pub state: CanaryBundleState,
    #[serde(default)]
    pub taint_reason: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub deployment_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub trace_id: String,
}

/// Tainted digest record included in `MuRegist` `/api/v1/canaries` envelope (`taints[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaintedDigestDto {
    pub digest: String,
    #[serde(default, alias = "service_name", deserialize_with = "null_default")]
    pub service: String,
    #[serde(default, deserialize_with = "null_default")]
    pub reason: String,
    #[serde(default, deserialize_with = "null_default")]
    pub trace_id: String,
}

/// `GET /api/reg/v1/cas/status` (`MuRegist` CAS storage health).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CasStatusDto {
    #[serde(default, alias = "total_blobs")]
    pub blob_count: u64,
    #[serde(default, alias = "total_bytes")]
    pub dedup_bytes: u64,
    #[serde(default)]
    pub raw_bytes: u64,
    #[serde(default)]
    pub dedup_savings_bytes: u64,
    #[serde(default, alias = "dedup_ratio_bps")]
    pub dedup_ratio_bp: u32,
    #[serde(default)]
    pub gc_watermark_pct: u32,
    #[serde(default)]
    pub last_scrub_epoch_ms: u64,
    #[serde(default, alias = "corrupted_blobs")]
    pub corrupt_blobs: u32,
}

impl CasStatusDto {
    /// Effective raw (un-deduplicated) byte count.
    #[must_use]
    pub fn effective_raw_bytes(&self) -> u64 {
        if self.raw_bytes > 0 {
            self.raw_bytes
        } else {
            self.dedup_bytes.saturating_add(self.dedup_savings_bytes)
        }
    }
}

// ---------------------------------------------------------------- Observe DTOs (MuObserve)

/// Node entry of [`SmartscapeDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartscapeNodeDto {
    pub id: String,
    #[serde(default, alias = "name", deserialize_with = "null_default")]
    pub label: String,
    #[serde(default, deserialize_with = "null_default")]
    pub layer: SmartscapeLayer,
    #[serde(default)]
    pub req_per_sec_bp: u32,
    #[serde(default)]
    pub request_rate_rps: f64,
    #[serde(default)]
    pub error_rate_bp: u32,
    #[serde(default)]
    pub error_rate_pct: f64,
    #[serde(default)]
    pub p95_ms: u32,
    #[serde(default)]
    pub p95_latency_ms: f64,
    #[serde(default = "default_true")]
    pub healthy: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub health_state: String,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn round_nonneg_u32(v: f64) -> u32 {
    if v <= 0.0 {
        0
    } else {
        (v + 0.5) as u32
    }
}

impl SmartscapeNodeDto {
    #[must_use]
    pub fn effective_req_per_sec_bp(&self) -> u32 {
        if self.req_per_sec_bp > 0 {
            self.req_per_sec_bp
        } else if self.request_rate_rps > 0.0 {
            round_nonneg_u32(self.request_rate_rps * 100.0)
        } else {
            0
        }
    }

    #[must_use]
    pub fn effective_error_rate_bp(&self) -> u32 {
        if self.error_rate_bp > 0 {
            self.error_rate_bp
        } else if self.error_rate_pct > 0.0 {
            round_nonneg_u32(self.error_rate_pct * 100.0)
        } else {
            0
        }
    }

    #[must_use]
    pub fn effective_p95_ms(&self) -> u32 {
        if self.p95_ms > 0 {
            self.p95_ms
        } else if self.p95_latency_ms > 0.0 {
            round_nonneg_u32(self.p95_latency_ms)
        } else {
            0
        }
    }

    #[must_use]
    pub fn effective_healthy(&self) -> bool {
        if !self.health_state.is_empty() {
            !matches!(
                self.health_state.to_ascii_lowercase().as_str(),
                "unhealthy" | "degraded" | "critical" | "error"
            )
        } else {
            self.healthy
        }
    }
}

const fn default_true() -> bool {
    true
}

/// Edge entry of [`SmartscapeDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartscapeEdgeDto {
    #[serde(alias = "source")]
    pub from_id: String,
    #[serde(alias = "target")]
    pub to_id: String,
    #[serde(default, alias = "relation", deserialize_with = "null_default")]
    pub protocol: String,
    #[serde(default)]
    pub call_rate_rps: u32,
    #[serde(default)]
    pub request_rate_rps: f64,
    #[serde(default)]
    pub error_rate_bp: u32,
    #[serde(default)]
    pub error_rate_pct: f64,
    #[serde(default)]
    pub p95_ms: u32,
    #[serde(default)]
    pub p95_latency_ms: f64,
}

impl SmartscapeEdgeDto {
    #[must_use]
    pub fn effective_call_rate_rps(&self) -> u32 {
        if self.call_rate_rps > 0 {
            self.call_rate_rps
        } else if self.request_rate_rps > 0.0 {
            round_nonneg_u32(self.request_rate_rps).max(1)
        } else {
            0
        }
    }

    #[must_use]
    pub fn effective_error_rate_bp(&self) -> u32 {
        if self.error_rate_bp > 0 {
            self.error_rate_bp
        } else if self.error_rate_pct > 0.0 {
            round_nonneg_u32(self.error_rate_pct * 100.0)
        } else {
            0
        }
    }

    #[must_use]
    pub fn effective_p95_ms(&self) -> u32 {
        if self.p95_ms > 0 {
            self.p95_ms
        } else if self.p95_latency_ms > 0.0 {
            round_nonneg_u32(self.p95_latency_ms)
        } else {
            0
        }
    }
}

/// `GET /api/obs/v1/smartscape`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SmartscapeDto {
    #[serde(default, deserialize_with = "null_default")]
    pub nodes: Vec<SmartscapeNodeDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub services: Vec<SmartscapeNodeDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub processes: Vec<SmartscapeNodeDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub hosts: Vec<SmartscapeNodeDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub edges: Vec<SmartscapeEdgeDto>,
}

impl SmartscapeDto {
    /// Return unified nodes list across `nodes` or `services` + `processes` + `hosts`.
    #[must_use]
    pub fn into_nodes_and_edges(self) -> (Vec<SmartscapeNodeDto>, Vec<SmartscapeEdgeDto>) {
        if !self.nodes.is_empty() {
            return (self.nodes, self.edges);
        }
        let mut all =
            Vec::with_capacity(self.services.len() + self.processes.len() + self.hosts.len());
        for mut n in self.services {
            n.layer = SmartscapeLayer::Service;
            all.push(n);
        }
        for mut n in self.processes {
            n.layer = SmartscapeLayer::Process;
            all.push(n);
        }
        for mut n in self.hosts {
            n.layer = SmartscapeLayer::Host;
            all.push(n);
        }
        (all, self.edges)
    }
}

/// Span entry of [`TraceDto`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSpanDto {
    pub span_id: String,
    #[serde(default)]
    pub parent_span_id: Option<String>,
    pub service: String,
    #[serde(default, alias = "span_name", deserialize_with = "null_default")]
    pub operation: String,
    #[serde(default)]
    pub start_offset_us: u64,
    #[serde(default)]
    pub duration_us: u64,
    #[serde(default)]
    pub self_time_us: u64,
    #[serde(default, alias = "is_critical_path")]
    pub critical_path: bool,
    #[serde(default)]
    pub error: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub status: String,
    #[serde(default, deserialize_with = "null_default")]
    pub status_code: String,
    #[serde(default, alias = "node_id", deserialize_with = "null_default")]
    pub hardware_class: String,
}

impl TraceSpanDto {
    #[must_use]
    pub fn effective_error(&self) -> bool {
        self.error
            || self.status.eq_ignore_ascii_case("error")
            || self.status_code.eq_ignore_ascii_case("error")
    }
}

/// `GET /api/obs/v1/traces` and `GET /api/obs/v1/traces/{trace_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceDto {
    pub trace_id: String,
    pub root_service: String,
    pub root_operation: String,
    #[serde(default, deserialize_with = "null_default")]
    pub domain: String,
    #[serde(default)]
    pub duration_ms: u32,
    #[serde(default)]
    pub duration_us: u64,
    #[serde(default)]
    pub error: bool,
    #[serde(default)]
    pub error_count: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub status: String,
    #[serde(default)]
    pub slow_p99: bool,
    #[serde(default, alias = "start_epoch_ms")]
    pub started_epoch_ms: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub spans: Vec<TraceSpanDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub correlated_logs: Vec<OtelLogDto>,
}

impl TraceDto {
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn effective_duration_ms(&self) -> u32 {
        if self.duration_ms > 0 {
            self.duration_ms
        } else if self.duration_us > 0 {
            ((self.duration_us + 500) / 1_000).max(1) as u32
        } else {
            0
        }
    }

    #[must_use]
    pub fn effective_error(&self) -> bool {
        self.error || self.error_count > 0 || self.status.eq_ignore_ascii_case("error")
    }
}

/// `GET /api/obs/v1/problems`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemDto {
    pub problem_id: String,
    pub title: String,
    #[serde(default, deserialize_with = "null_default")]
    pub category: ProblemCategory,
    #[serde(default, deserialize_with = "null_default")]
    pub severity: String,
    #[serde(default)]
    pub burn_rate_bp: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub impacted_entity: String,
    #[serde(default, deserialize_with = "null_default")]
    pub root_cause_trace_id: String,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub status: String,
}

impl ProblemDto {
    #[must_use]
    pub fn effective_active(&self) -> bool {
        if !self.status.is_empty() {
            !matches!(
                self.status.to_ascii_lowercase().as_str(),
                "closed" | "resolved" | "inactive"
            )
        } else {
            self.active
        }
    }
}

/// `GET /api/obs/v1/logs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtelLogDto {
    #[serde(default)]
    pub timestamp_ms: u64,
    #[serde(default, alias = "severity_text", deserialize_with = "null_default")]
    pub severity: LogSeverity,
    pub service: String,
    #[serde(default)]
    pub span_id: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub trace_id: String,
    #[serde(default, alias = "body", deserialize_with = "null_default")]
    pub message: String,
}

/// `GET /api/obs/v1/metrics/timeseries`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricSeriesDto {
    pub service: String,
    #[serde(default, deserialize_with = "null_default")]
    pub hardware_class: String,
    #[serde(default)]
    pub rps: u32,
    #[serde(default)]
    pub error_rate_bp: u32,
    #[serde(default)]
    pub p50_ms: u32,
    #[serde(default)]
    pub p95_ms: u32,
    #[serde(default)]
    pub p99_ms: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub exemplar_trace_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub samples: Vec<u32>,
}

const fn abs_f64(v: f64) -> f64 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

/// Raw metric point or pre-aggregated series item from `GET /api/obs/v1/metrics/timeseries`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RawMetricPointDto {
    #[serde(default, deserialize_with = "null_default")]
    pub timestamp_ns: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub service: String,
    #[serde(default, deserialize_with = "null_default")]
    pub node_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub metric_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub metric_unit: String,
    #[serde(default, deserialize_with = "null_default")]
    pub value_f64: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub arch: String,
    #[serde(default, deserialize_with = "null_default")]
    pub cpu_cluster: String,
    #[serde(default, deserialize_with = "null_default")]
    pub vlen_bits: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub hardware_class: String,
    #[serde(default, deserialize_with = "null_default")]
    pub rps: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub error_rate_bp: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub p50_ms: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub p95_ms: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub p99_ms: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub exemplar_trace_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub samples: Vec<u32>,
}

impl RawMetricPointDto {
    /// Whether this item represents a raw OTel metric point rather than a pre-aggregated series.
    #[must_use]
    pub fn is_raw_point(&self) -> bool {
        !self.metric_name.is_empty()
            || (self.hardware_class.is_empty()
                && self.samples.is_empty()
                && (!self.node_id.is_empty() || !self.arch.is_empty() || self.value_f64 != 0.0))
    }

    /// Aggregate a slice of raw metric points and/or pre-aggregated series into [`MetricSeriesDto`] rows.
    #[must_use]
    pub fn aggregate_into_series(points: &[Self]) -> Vec<MetricSeriesDto> {
        type GroupKey<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str, &'a str, u32);
        let mut out = Vec::new();
        let mut groups: Vec<(GroupKey<'_>, Vec<&Self>)> = Vec::new();

        for item in points {
            if !item.is_raw_point()
                && (!item.hardware_class.is_empty()
                    || !item.samples.is_empty()
                    || item.p50_ms > 0
                    || item.p95_ms > 0
                    || item.p99_ms > 0
                    || item.rps > 0)
            {
                out.push(MetricSeriesDto {
                    service: item.service.clone(),
                    hardware_class: item.hardware_class.clone(),
                    rps: item.rps,
                    error_rate_bp: item.error_rate_bp,
                    p50_ms: item.p50_ms,
                    p95_ms: item.p95_ms,
                    p99_ms: item.p99_ms,
                    exemplar_trace_id: item.exemplar_trace_id.clone(),
                    samples: item.samples.clone(),
                });
                continue;
            }
            if !item.is_raw_point() {
                continue;
            }
            let key: GroupKey<'_> = (
                item.service.as_str(),
                item.metric_name.as_str(),
                item.metric_unit.as_str(),
                item.node_id.as_str(),
                item.arch.as_str(),
                item.cpu_cluster.as_str(),
                item.vlen_bits,
            );
            if let Some((_, bucket)) = groups.iter_mut().find(|(k, _)| *k == key) {
                bucket.push(item);
            } else {
                groups.push((key, alloc::vec![item]));
            }
        }

        for (key, mut group_points) in groups {
            if group_points.iter().any(|p| p.timestamp_ns > 0) {
                group_points.sort_by_key(|p| p.timestamp_ns);
            }
            let (svc, m_name, m_unit, node_id, arch, cpu_cluster, vlen_bits) = key;
            let service = match (svc.is_empty(), m_name.is_empty(), m_unit.is_empty()) {
                (true, true, _) => String::from("telemetry"),
                (true, false, true) => String::from(m_name),
                (true, false, false) => format!("{m_name} ({m_unit})"),
                (false, true, _) => String::from(svc),
                (false, false, true) => format!("{svc} · {m_name}"),
                (false, false, false) => format!("{svc} · {m_name} ({m_unit})"),
            };

            let mut hw_parts: Vec<String> = Vec::new();
            if !node_id.is_empty() {
                hw_parts.push(String::from(node_id));
            }
            if !arch.is_empty() {
                hw_parts.push(String::from(arch));
            }
            if !cpu_cluster.is_empty() && !cpu_cluster.eq_ignore_ascii_case("unknown") {
                hw_parts.push(String::from(cpu_cluster));
            }
            if vlen_bits > 0 {
                hw_parts.push(format!("rvv-{vlen_bits}"));
            }
            let hardware_class = if hw_parts.is_empty() {
                String::from("cluster")
            } else {
                hw_parts.join(" · ")
            };

            let max_val = group_points.iter().fold(0.0_f64, |acc, p| {
                let v = if p.value_f64.is_nan() {
                    0.0
                } else {
                    abs_f64(p.value_f64)
                };
                if v > acc {
                    v
                } else {
                    acc
                }
            });
            let scale_100 = max_val > 0.0 && max_val < 10.0;
            let total_points = group_points.len();
            let tail_start = total_points.saturating_sub(24);
            let samples: Vec<u32> = group_points[tail_start..]
                .iter()
                .map(|p| {
                    let v = if p.value_f64.is_nan() {
                        0.0
                    } else {
                        abs_f64(p.value_f64)
                    };
                    if scale_100 {
                        round_nonneg_u32(v * 100.0)
                    } else {
                        round_nonneg_u32(v)
                    }
                })
                .collect();

            let rps = u32::try_from(total_points).unwrap_or(u32::MAX);
            let mut sorted = samples.clone();
            sorted.sort_unstable();
            let (p50_ms, p95_ms, p99_ms) = if sorted.is_empty() {
                (0, 0, 0)
            } else {
                let len = sorted.len();
                let p50 = sorted[len / 2];
                let p95 = sorted[(len * 95 / 100).min(len - 1)];
                let p99 = sorted[(len * 99 / 100).min(len - 1)];
                (p50, p95, p99)
            };
            let error_rate_bp = group_points
                .iter()
                .map(|p| p.error_rate_bp)
                .max()
                .unwrap_or(0);
            let exemplar_trace_id = group_points
                .iter()
                .find(|p| !p.exemplar_trace_id.is_empty())
                .map_or_else(String::new, |p| p.exemplar_trace_id.clone());

            out.push(MetricSeriesDto {
                service,
                hardware_class,
                rps,
                error_rate_bp,
                p50_ms,
                p95_ms,
                p99_ms,
                exemplar_trace_id,
                samples,
            });
        }

        out
    }
}

/// `GET /api/obs/v1/store/status`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StoreStatusDto {
    #[serde(default)]
    pub hot_duckdb_rows: u64,
    #[serde(default)]
    pub hot_spans: u64,
    #[serde(default)]
    pub hot_logs: u64,
    #[serde(default)]
    pub hot_metric_points: u64,
    #[serde(default)]
    pub hot_duckdb_bytes: u64,
    #[serde(default, alias = "parquet_segments")]
    pub cold_parquet_partitions: u32,
    #[serde(default)]
    pub cold_parquet_rows: u64,
    #[serde(default)]
    pub cold_spans: u64,
    #[serde(default)]
    pub cold_logs: u64,
    #[serde(default)]
    pub cold_metric_points: u64,
    #[serde(default)]
    pub cold_parquet_bytes: u64,
    #[serde(default)]
    pub zstd_compression_bp: u32,
    #[serde(default)]
    pub estimated_compression_ratio: f64,
    #[serde(default)]
    pub retention_days: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub oldest_watermark_iso: String,
}

impl StoreStatusDto {
    #[must_use]
    pub fn effective_hot_rows(&self) -> u64 {
        if self.hot_duckdb_rows > 0 {
            self.hot_duckdb_rows
        } else {
            self.hot_spans
                .saturating_add(self.hot_logs)
                .saturating_add(self.hot_metric_points)
        }
    }

    #[must_use]
    pub fn effective_cold_rows(&self) -> u64 {
        if self.cold_parquet_rows > 0 {
            self.cold_parquet_rows
        } else {
            self.cold_spans
                .saturating_add(self.cold_logs)
                .saturating_add(self.cold_metric_points)
        }
    }

    #[must_use]
    pub fn effective_zstd_compression_bp(&self) -> u32 {
        if self.zstd_compression_bp > 0 {
            self.zstd_compression_bp
        } else if self.estimated_compression_ratio > 0.0 {
            round_nonneg_u32(self.estimated_compression_ratio * 100.0)
        } else {
            100
        }
    }
}

/// `GET /api/cluster/nodes` entry (`ActionRunner-controller`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusterNodeDto {
    pub node_id: String,
    #[serde(default, alias = "online")]
    pub is_online: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub arch: String,
    #[serde(default)]
    pub active_slots: u32,
    #[serde(default)]
    pub max_slots: u32,
    #[serde(default)]
    pub cpu_load_pct: f64,
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
    #[serde(default, deserialize_with = "null_default")]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub toolchains: Vec<String>,
}

impl ClusterNodeDto {
    /// Convert `cpu_load_pct` (`0.0..=100.0`) to basis points (`0..=10_000`).
    #[must_use]
    pub fn cpu_load_bp(&self) -> u32 {
        round_nonneg_u32(self.cpu_load_pct * 100.0).min(10_000)
    }
}

/// `job` nested object inside [`CiQueueJobRecordDto`] (`GET /api/queue`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CiQueueJobSpecDto {
    #[serde(default, deserialize_with = "null_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub repo_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub ref_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub commit_sha: String,
    #[serde(default, deserialize_with = "null_default")]
    pub event_type: String,
    #[serde(default, deserialize_with = "null_default")]
    pub queued_at: String,
}

/// `metrics` nested object inside [`CiQueueJobRecordDto`] (`GET /api/queue`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CiQueueMetricsDto {
    #[serde(default)]
    pub quality_score: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub letter_grade: String,
    #[serde(default)]
    pub coverage_pct: f64,
    #[serde(default)]
    pub tests_passed: u32,
    #[serde(default)]
    pub tests_total: u32,
    #[serde(default)]
    pub clippy_warnings: u32,
    #[serde(default)]
    pub advisories_count: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub benchmark_summary: String,
}

/// `status.details` nested object inside [`CiQueueStageStatusDto`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CiQueueStageDetailsDto {
    #[serde(default)]
    pub duration_secs: u64,
    #[serde(default)]
    pub exit_code: i32,
}

/// `status` nested object inside [`CiQueueStageDto`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CiQueueStageStatusDto {
    #[serde(default, deserialize_with = "null_default")]
    pub state: String,
    #[serde(default, deserialize_with = "null_default")]
    pub details: CiQueueStageDetailsDto,
}

/// `stages[]` entry inside [`CiQueueJobRecordDto`] (`GET /api/queue`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CiQueueStageDto {
    #[serde(default, deserialize_with = "null_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default)]
    pub assigned_node: Option<String>,
    #[serde(default)]
    pub threads: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub status: CiQueueStageStatusDto,
}

/// Nested `record` wrapper inside `active_jobs` / `queued_jobs` (`QueueItemView`) of `GET /api/queue`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CiQueueJobInnerDto {
    #[serde(default, deserialize_with = "null_default")]
    pub job: CiQueueJobSpecDto,
    #[serde(default, deserialize_with = "null_default")]
    pub status: CiQueueStageStatusDto,
    #[serde(default)]
    pub metrics: Option<CiQueueMetricsDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub stages: Vec<CiQueueStageDto>,
}

/// One job record inside `active_jobs`, `queued_jobs`, or `recent_jobs` of `GET /api/queue`,
/// or a single `GET /api/jobs/{id}` response.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CiQueueJobRecordDto {
    #[serde(default, deserialize_with = "null_default")]
    pub job: CiQueueJobSpecDto,
    #[serde(default, deserialize_with = "null_default")]
    pub status: CiQueueStageStatusDto,
    #[serde(default)]
    pub metrics: Option<CiQueueMetricsDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub stages: Vec<CiQueueStageDto>,
    #[serde(default)]
    pub elapsed_secs: u64,
    #[serde(default)]
    pub wait_secs: u64,
    #[serde(default)]
    pub position: u32,
    #[serde(default)]
    pub record: Option<CiQueueJobInnerDto>,
}

impl CiQueueJobRecordDto {
    /// Unwrap nested `QueueItemView.record` if present (`active_jobs` and `queued_jobs`).
    #[must_use]
    pub fn into_unwrapped(mut self) -> Self {
        if let Some(inner) = self.record.take() {
            if self.job.id.is_empty() {
                self.job = inner.job;
            }
            if self.status.state.is_empty() {
                self.status = inner.status;
            }
            if self.metrics.is_none() {
                self.metrics = inner.metrics;
            }
            if self.stages.is_empty() {
                self.stages = inner.stages;
            }
        }
        self
    }
}

/// `GET /api/queue` response (`ActionRunner-controller`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CiQueueDto {
    #[serde(default)]
    pub queue_depth: u32,
    #[serde(default)]
    pub concurrency: u32,
    #[serde(default, deserialize_with = "null_default")]
    pub active_jobs: Vec<CiQueueJobRecordDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub queued_jobs: Vec<CiQueueJobRecordDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub recent_jobs: Vec<CiQueueJobRecordDto>,
}

/// `GET /api/auth/session` and `POST /api/auth/redeem` response (`ActionRunner-controller`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuthSessionDto {
    #[serde(default)]
    pub authenticated: bool,
    #[serde(default)]
    pub github_login: Option<String>,
    #[serde(default)]
    pub invite_bound: bool,
    #[serde(default)]
    pub role: Option<String>,
}

/// Dynamic reverse-proxy route definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngressRouteDto {
    pub route_id: String,
    pub path_prefix: String,
    pub upstream_url: String,
    #[serde(default)]
    pub service_name: String,
    #[serde(default)]
    pub strip_prefix: bool,
    #[serde(default)]
    pub auth_required: bool,
    #[serde(default)]
    pub request_count: u64,
    #[serde(default)]
    pub request_rate_rps: f64,
    #[serde(default)]
    pub error_rate_pct: f64,
    #[serde(default)]
    pub p95_latency_ms: f64,
}

impl Eq for IngressRouteDto {}

/// Upstream target host and health telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngressUpstreamDto {
    pub upstream_id: String,
    pub target_url: String,
    #[serde(default)]
    pub service_name: String,
    #[serde(default)]
    pub health_state: String, // "HEALTHY", "DEGRADED", "UNHEALTHY"
    #[serde(default)]
    pub latency_ms: f64,
    #[serde(default)]
    pub success_rate_pct: f64,
    #[serde(default)]
    pub active_connections: u32,
    #[serde(default)]
    pub last_check_ms: i64,
}

impl Eq for IngressUpstreamDto {}

/// TLS termination and certificate health status.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TlsHealthDto {
    pub domain: String,
    pub certificate_issuer: String,
    #[serde(default)]
    pub valid_until_epoch_ms: i64,
    #[serde(default)]
    pub days_remaining: i32,
    #[serde(default)]
    pub alpn_protocols: Vec<String>,
    #[serde(default)]
    pub ocsp_stapling: bool,
    #[serde(default)]
    pub mutual_tls: bool,
}

/// Aggregate Ingress overview bundle.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IngressOverviewDto {
    #[serde(default)]
    pub routes: Vec<IngressRouteDto>,
    #[serde(default)]
    pub upstreams: Vec<IngressUpstreamDto>,
    #[serde(default)]
    pub tls_health: Option<TlsHealthDto>,
}

impl Eq for IngressOverviewDto {}

/// REST paths (relative to the configured API base).
pub mod paths {
    use alloc::format;
    use alloc::string::String;

    /// CI event stream (existing endpoint, new event kinds).
    pub const CI_EVENTS: &str = "/api/events";
    /// CD event stream.
    pub const CD_EVENTS: &str = "/api/cd/events";
    /// Repository list.
    pub const REPOS: &str = "/api/repos";
    /// Cluster fabric worker nodes (`ActionRunner-controller`).
    pub const CLUSTER_NODES: &str = "/api/cluster/nodes";
    /// Cluster-wide CI job queue & recent execution records (`ActionRunner-controller`).
    pub const CI_QUEUE: &str = "/api/queue";
    /// Active CD deployment leases.
    pub const LEASES: &str = "/api/cd/leases";
    /// Operator authentication session probe.
    pub const AUTH_SESSION: &str = "/api/auth/session";
    /// One-time invite token redemption endpoint.
    pub const AUTH_REDEEM: &str = "/api/auth/redeem";

    /// Registry package catalog (`MuRegist`).
    pub const REG_PACKAGES: &str = "/api/reg/v1/packages";
    /// Registry canary bundles & taint provenance (`MuRegist`).
    pub const REG_CANARIES: &str = "/api/reg/v1/canaries";
    /// Registry CAS storage health (`MuRegist`).
    pub const REG_CAS_STATUS: &str = "/api/reg/v1/cas/status";

    /// Observe Smartscape 3-tier topology (`MuObserve`).
    pub const OBS_SMARTSCAPE: &str = "/api/obs/v1/smartscape";
    /// Observe causal SLO problems (`MuObserve`).
    pub const OBS_PROBLEMS: &str = "/api/obs/v1/problems";
    /// Observe recent PurePath traces (`MuObserve`).
    pub const OBS_TRACES: &str = "/api/obs/v1/traces";
    /// Observe sanitized OTel log stream (`MuObserve`).
    pub const OBS_LOGS: &str = "/api/obs/v1/logs";
    /// Observe golden-signal & hardware metric series (`MuObserve`).
    pub const OBS_METRICS: &str = "/api/obs/v1/metrics/timeseries";
    /// Observe DuckDB + Parquet storage health (`MuObserve`).
    pub const OBS_STORE_STATUS: &str = "/api/obs/v1/store/status";
    /// Observe persistent SSE stream (`MuObserve`).
    pub const OBS_EVENTS: &str = "/api/obs/v1/events";
    /// Observe ingress overview (`MuObserve`).
    pub const OBS_INGRESS_OVERVIEW: &str = "/api/obs/v1/ingress/overview";
    /// Observe dynamic ingress routes (`MuObserve`).
    pub const OBS_INGRESS_ROUTES: &str = "/api/obs/v1/ingress/routes";
    /// Observe ingress upstreams health (`MuObserve`).
    pub const OBS_INGRESS_UPSTREAMS: &str = "/api/obs/v1/ingress/upstreams";

    /// `/api/obs/v1/logs?q={q}&limit={limit}`.
    #[must_use]
    pub fn obs_logs_query(q: &str, limit: usize) -> String {
        format!("/api/obs/v1/logs?q={}&limit={}", q, limit)
    }

    /// `/api/repos/{owner}/{repo}/summary`.
    #[must_use]
    pub fn summary(owner: &str, repo: &str) -> String {
        format!("/api/repos/{owner}/{repo}/summary")
    }

    /// `/api/repos/{owner}/{repo}/topology`.
    #[must_use]
    pub fn topology(owner: &str, repo: &str) -> String {
        format!("/api/repos/{owner}/{repo}/topology")
    }

    /// `/api/cd/deployments?repo={owner}/{repo}&limit={limit}`.
    #[must_use]
    pub fn deployments(owner: &str, repo: &str, limit: u32) -> String {
        format!("/api/cd/deployments?repo={owner}/{repo}&limit={limit}")
    }

    /// `/api/cd/deployments/{id}/gate`.
    #[must_use]
    pub fn gate(deployment_id: &str) -> String {
        format!("/api/cd/deployments/{deployment_id}/gate")
    }

    /// `/api/obs/v1/traces/{trace_id}`.
    #[must_use]
    pub fn obs_trace(trace_id: &str) -> String {
        format!("/api/obs/v1/traces/{trace_id}")
    }

    /// `/api/jobs/{job_id}/restart` (`POST` on `ActionRunner-controller`).
    #[must_use]
    pub fn job_restart(job_id: u64) -> String {
        format!("/api/jobs/{job_id}/restart")
    }

    /// `/api/jobs/{job_id}/restart` using the canonical string job ID (`"job-{ts}-{sha8}"`).
    #[must_use]
    pub fn job_restart_id(job_id: &str) -> String {
        format!("/api/jobs/{job_id}/restart")
    }

    /// `/api/jobs/{job_id}/resume` (`POST` on `ActionRunner-controller`, preserving completed stages).
    #[must_use]
    pub fn job_resume_id(job_id: &str) -> String {
        format!("/api/jobs/{job_id}/resume")
    }

    /// `/api/jobs/{job_id}` (`GET` structured job detail & per-stage statuses).
    #[must_use]
    pub fn job_detail_id(job_id: &str) -> String {
        format!("/api/jobs/{job_id}")
    }

    /// `/api/jobs/{job_id}/log` (`GET` raw build log text).
    #[must_use]
    pub fn job_log_id(job_id: &str) -> String {
        format!("/api/jobs/{job_id}/log")
    }

    /// `/api/obs/v1/jobs/{job_id}/log` (`GET` sanitized public build log text).
    #[must_use]
    pub fn job_obs_log_id(job_id: &str) -> String {
        format!("/api/obs/v1/jobs/{job_id}/log")
    }
}
