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

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::{Area, AuthReasonCode, ConsoleSkin, CrtFont, RepoId, RoleLeaseView, SkipReason};
pub use crate::view::{
    CanaryBundleState, GateOutcome, LogSeverity, PackageEcosystem, PrState, ProblemCategory,
    RepoStatus, RollbackReason, Slot, SmartscapeLayer, StageState, TestState, Visibility,
};

fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    if d.is_human_readable() {
        Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
    } else {
        T::deserialize(d)
    }
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
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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
    #[serde(default)]
    pub causal_chain: Vec<String>,
    #[serde(default)]
    pub confidence_score: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub root_cause_service: String,
    #[serde(default)]
    pub root_cause_span_id: Option<String>,
    #[serde(default)]
    pub root_cause_error_message: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub summary: String,
    #[serde(default)]
    pub affected_services: Vec<String>,
    #[serde(default)]
    pub ai_diagnosis: Option<String>,
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
    #[serde(default)]
    pub skip_reason: Option<SkipReason>,
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
    #[serde(default)]
    pub skip_reason: Option<SkipReason>,
}

impl CiQueueStageDto {
    /// Return structured skip reason from either stage-level or nested `status.details`.
    #[must_use]
    pub fn effective_skip_reason(&self) -> Option<&SkipReason> {
        self.skip_reason
            .as_ref()
            .or(self.status.details.skip_reason.as_ref())
    }
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

/// `GET /api/auth/session` and `POST /api/auth/redeem` response (`mu-auth` / `ActionRunner-controller`).
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
    #[serde(default)]
    pub reason_code: Option<AuthReasonCode>,
    #[serde(default)]
    pub passkey_count: u32,
    #[serde(default)]
    pub passkey_enrollment_required: bool,
    #[serde(default)]
    pub anonymous_lockdown: bool,
    #[serde(default)]
    pub baseline_role: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub bound_roles: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub active_roles: Vec<RoleSpecDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub leases: Vec<RoleLeaseView>,
    #[serde(default, deserialize_with = "null_default")]
    pub active_folios: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub active_policies: Vec<String>,
    #[serde(default)]
    pub preferences: Option<UserPreferencesDto>,
    #[serde(default)]
    pub display_name: Option<String>,
}

// ---------------------------------------------------------------- mu-auth & Platform DTOs

/// Canonical 8 platform subsystems mirrored 1:1 with `mu-auth::SubsystemId`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SubsystemId {
    #[default]
    System,
    Ci,
    Cd,
    #[serde(alias = "ab-deploy", alias = "ab_deploy")]
    AbDeploy,
    Repository,
    Observability,
    #[serde(alias = "model-runner", alias = "model_runner")]
    ModelRunner,
    Platform,
}

impl SubsystemId {
    /// All 8 subsystems including [`Self::System`].
    pub const ALL: [Self; 8] = [
        Self::System,
        Self::Ci,
        Self::Cd,
        Self::AbDeploy,
        Self::Repository,
        Self::Observability,
        Self::ModelRunner,
        Self::Platform,
    ];

    /// The 7 operational platform subsystems tracked for health & maintenance status.
    pub const OPERATIONAL: [Self; 7] = [
        Self::Ci,
        Self::Cd,
        Self::AbDeploy,
        Self::Repository,
        Self::Observability,
        Self::ModelRunner,
        Self::Platform,
    ];

    /// Canonical `snake_case` subsystem identifier string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Ci => "ci",
            Self::Cd => "cd",
            Self::AbDeploy => "ab_deploy",
            Self::Repository => "repository",
            Self::Observability => "observability",
            Self::ModelRunner => "model_runner",
            Self::Platform => "platform",
        }
    }

    /// Canonical policy namespace prefix (`<prefix>:<resource>.<action>`).
    #[must_use]
    pub const fn policy_prefix(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Ci => "ci",
            Self::Cd => "cd",
            Self::AbDeploy => "ab",
            Self::Repository => "reg",
            Self::Observability => "obs",
            Self::ModelRunner => "mr",
            Self::Platform => "platform",
        }
    }

    /// Resolve the [`Area`] mapped 1:1 to this subsystem.
    #[must_use]
    pub const fn area(self) -> Area {
        match self {
            Self::System => Area::Dashboard,
            Self::Ci => Area::Ci,
            Self::Cd => Area::Cd,
            Self::AbDeploy => Area::AbDeploy,
            Self::Repository => Area::Repository,
            Self::Observability => Area::Observability,
            Self::ModelRunner => Area::ModelRunner,
            Self::Platform => Area::Platform,
        }
    }

    /// Resolve a subsystem from its canonical policy prefix.
    #[must_use]
    pub fn from_policy_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "system" => Some(Self::System),
            "ci" => Some(Self::Ci),
            "cd" => Some(Self::Cd),
            "ab" => Some(Self::AbDeploy),
            "reg" => Some(Self::Repository),
            "obs" => Some(Self::Observability),
            "mr" => Some(Self::ModelRunner),
            "platform" => Some(Self::Platform),
            _ => None,
        }
    }
}

impl fmt::Display for SubsystemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SubsystemId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "system" => Ok(Self::System),
            "ci" => Ok(Self::Ci),
            "cd" => Ok(Self::Cd),
            "ab_deploy" | "ab-deploy" | "ab" => Ok(Self::AbDeploy),
            "repository" | "reg" => Ok(Self::Repository),
            "observability" | "obs" => Ok(Self::Observability),
            "model_runner" | "model-runner" | "mr" => Ok(Self::ModelRunner),
            "platform" | "iam" => Ok(Self::Platform),
            _ => Err(()),
        }
    }
}

/// Generic 5-state operational health classification mirrored 1:1 with `mu-auth::SubsystemStatus`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum SubsystemStatus {
    #[default]
    Live,
    #[serde(alias = "in_maintenance")]
    InMaintenance,
    #[serde(alias = "partial_maintenance")]
    PartialMaintenance,
    Offline,
    Degraded,
}

impl SubsystemStatus {
    /// Canonical `kebab-case` wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::InMaintenance => "in-maintenance",
            Self::PartialMaintenance => "partial-maintenance",
            Self::Offline => "offline",
            Self::Degraded => "degraded",
        }
    }

    /// Operator-facing status label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Live => "Live",
            Self::InMaintenance => "In Maintenance",
            Self::PartialMaintenance => "Partial Maintenance",
            Self::Offline => "Offline",
            Self::Degraded => "Degraded",
        }
    }

    /// Return `true` when scheduled or partial maintenance is active.
    #[must_use]
    pub const fn is_maintenance(self) -> bool {
        matches!(self, Self::InMaintenance | Self::PartialMaintenance)
    }

    /// Return `true` when outage SLO problem cards should be suppressed.
    #[must_use]
    pub const fn suppresses_slo_alerts(self) -> bool {
        matches!(
            self,
            Self::InMaintenance | Self::PartialMaintenance | Self::Offline
        )
    }
}

impl fmt::Display for SubsystemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SubsystemStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "live" => Ok(Self::Live),
            "in-maintenance" | "in_maintenance" => Ok(Self::InMaintenance),
            "partial-maintenance" | "partial_maintenance" => Ok(Self::PartialMaintenance),
            "offline" => Ok(Self::Offline),
            "degraded" => Ok(Self::Degraded),
            _ => Err(()),
        }
    }
}

/// Operator maintenance control mode mirrored 1:1 with `mu-auth::MaintenanceMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaintenanceMode {
    #[default]
    Auto,
    Manual(SubsystemStatus),
}

/// Manual maintenance override mirrored 1:1 with `mu-auth::MaintenanceOverride`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MaintenanceOverrideDto {
    pub subsystem: SubsystemId,
    #[serde(default)]
    pub status: SubsystemStatus,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub updated_by: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub auto_clear_at_ms: Option<u64>,
}

/// Type alias matching `mu-auth::MaintenanceOverride`.
pub type MaintenanceOverride = MaintenanceOverrideDto;

/// Step-up verification requirement mirrored 1:1 with `mu-auth::StepUpRequirement`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum StepUpRequirement {
    #[default]
    None,
    Passkey,
}

/// Discriminant kind of [`RoleActivationDto`] matching `mu-auth::RoleActivation`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RoleActivationKind {
    #[default]
    PinnedBaseline,
    Elective,
    Timed,
}

/// Role activation lifecycle matching `mu-auth::RoleActivation`'s JSON tag format (`{"kind": ...}`)
/// while remaining natively compatible with non-self-describing `postcard` frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoleActivationDto {
    pub kind: RoleActivationKind,
    #[serde(default)]
    pub hard_ttl_secs: Option<u64>,
    #[serde(default)]
    pub idle_ttl_secs: Option<u64>,
}

impl RoleActivationDto {
    /// Canonical `PinnedBaseline` activation descriptor.
    #[must_use]
    pub const fn pinned_baseline() -> Self {
        Self {
            kind: RoleActivationKind::PinnedBaseline,
            hard_ttl_secs: None,
            idle_ttl_secs: None,
        }
    }

    /// Dormant `Elective` role activation descriptor.
    #[must_use]
    pub const fn elective() -> Self {
        Self {
            kind: RoleActivationKind::Elective,
            hard_ttl_secs: None,
            idle_ttl_secs: None,
        }
    }

    /// Fixed server-bounded `Timed` activation descriptor.
    #[must_use]
    pub const fn timed(hard_ttl_secs: u64, idle_ttl_secs: u64) -> Self {
        Self {
            kind: RoleActivationKind::Timed,
            hard_ttl_secs: Some(hard_ttl_secs),
            idle_ttl_secs: Some(idle_ttl_secs),
        }
    }
}

/// Type alias matching `mu-auth::RoleActivation`.
pub type RoleActivation = RoleActivationDto;

/// Role specification DTO mirrored 1:1 with `mu-auth::RoleSpec`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoleSpecDto {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    #[serde(default)]
    pub activation: RoleActivationDto,
    #[serde(default)]
    pub step_up: StepUpRequirement,
}

impl RoleSpecDto {
    /// Return human-facing display name when present, otherwise fallback to immutable role ID.
    #[must_use]
    pub fn effective_display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(self.id.as_str())
    }
}

/// Re-exported DTO alias for `mu-auth::RoleSpec`.
pub type RoleSpec = RoleSpecDto;

/// Provenance ownership of a [`PolicyFolioDto`], mirrored 1:1 with `mu-auth::FolioOrigin`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum FolioOrigin {
    Subsystem,
    #[default]
    Operator,
}

/// Atomic policy descriptor mirrored 1:1 with `mu-auth::Policy`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PolicyDto {
    pub id: String,
    pub subsystem: SubsystemId,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
}

/// Composable positive-grant PolicyFolio DTO mirrored 1:1 with `mu-auth::PolicyFolio`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PolicyFolioDto {
    pub id: String,
    pub display_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    #[serde(default)]
    pub origin: FolioOrigin,
    #[serde(default)]
    pub owner_subsystem: Option<SubsystemId>,
    #[serde(default, deserialize_with = "null_default")]
    pub includes: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub policies: Vec<String>,
}

/// Re-exported DTO alias for `mu-auth::PolicyFolio`.
pub type PolicyFolio = PolicyFolioDto;

/// Subsystem security manifest DTO mirrored 1:1 with `mu-auth::SubsystemSecurityManifest`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubsystemSecurityManifestDto {
    pub subsystem: SubsystemId,
    #[serde(default, deserialize_with = "null_default")]
    pub policies: Vec<PolicyDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub folios: Vec<PolicyFolioDto>,
}

/// Role-to-folio indirection table mirrored 1:1 with `mu-auth::RoleFolioIndex`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoleFolioIndexDto {
    #[serde(default, deserialize_with = "null_default")]
    pub anonymous: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub role_folios: BTreeMap<String, Vec<String>>,
}

/// Re-exported DTO alias for `mu-auth::RoleFolioIndex`.
pub type RoleFolioIndex = RoleFolioIndexDto;

/// Per-credential WebAuthn Passkey metadata returned to the `@user` Profile modal.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PasskeySummaryDto {
    pub credential_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub sign_count: u32,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub last_used_at_ms: Option<u64>,
}

/// Persisted UI skin & accessibility font preferences attached to a user profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserPreferencesDto {
    #[serde(default)]
    pub skin: ConsoleSkin,
    #[serde(default)]
    pub font: CrtFont,
}

/// `GET/PUT /api/auth/profile` response (`@user` profile, preferences & enrolled Passkeys).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserProfileDto {
    pub github_login: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub preferences: UserPreferencesDto,
    #[serde(default, deserialize_with = "null_default")]
    pub bound_roles: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub passkeys: Vec<PasskeySummaryDto>,
    #[serde(default)]
    pub passkey_count: u32,
}

/// Bound platform user summary listed in `Platform > Users & Invites`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlatformUserDto {
    pub github_login: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub bound_roles: Vec<String>,
    #[serde(default)]
    pub passkey_count: u32,
    #[serde(default)]
    pub last_seen_ms: Option<u64>,
    #[serde(default)]
    pub is_platform_admin: bool,
}

/// Onboarding invite record listed in `Platform > Users & Invites`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InviteRecordDto {
    pub invite_id: String,
    pub granted_role: String,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub redeemed_by: Option<String>,
    #[serde(default)]
    pub redeemed_at_ms: Option<u64>,
    #[serde(default)]
    pub revoked: bool,
}

/// Single-display response from `POST /api/auth/invites`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CreatedInviteResponseDto {
    pub invite_id: String,
    pub token: String,
    pub url: String,
    pub granted_role: String,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Sanitized platform authorization / audit activity entry.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuditActivityEntryDto {
    pub timestamp_ms: u64,
    pub actor: String,
    pub action: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub outcome: String,
}

/// Combined platform configuration & RBAC snapshot (`Area::Platform`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlatformSnapshotDto {
    #[serde(default, deserialize_with = "null_default")]
    pub subsystems: Vec<SubsystemStatusEntryDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub users: Vec<PlatformUserDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub invites: Vec<InviteRecordDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub roles: Vec<RoleSpecDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub folios: Vec<PolicyFolioDto>,
    #[serde(default, deserialize_with = "null_default")]
    pub policies: Vec<PolicyDto>,
    #[serde(default)]
    pub role_folio_index: RoleFolioIndexDto,
    #[serde(default, deserialize_with = "null_default")]
    pub recent_activity: Vec<AuditActivityEntryDto>,
}

// ---------------------------------------------------------------- 7-Subsystem Health DTOs

/// One subsystem status row returned by `GET /api/obs/v1/status/subsystems`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubsystemStatusEntryDto {
    pub subsystem: SubsystemId,
    #[serde(default)]
    pub effective_status: SubsystemStatus,
    #[serde(default)]
    pub probe_status: SubsystemStatus,
    #[serde(default)]
    pub mode: MaintenanceMode,
    #[serde(default, deserialize_with = "null_default")]
    pub summary: String,
    #[serde(default)]
    pub updated_by: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub auto_clear_at_ms: Option<u64>,
    #[serde(default)]
    pub active_incidents: u32,
}

/// Per-subsystem health record stored inside [`SubsystemHealthSummary`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubsystemHealthEntry {
    #[serde(default)]
    pub status: SubsystemStatus,
    #[serde(default)]
    pub probe_status: SubsystemStatus,
    #[serde(default)]
    pub mode: MaintenanceMode,
    #[serde(default, deserialize_with = "null_default")]
    pub summary: String,
    #[serde(default)]
    pub updated_by: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default)]
    pub auto_clear_at_ms: Option<u64>,
    #[serde(default)]
    pub active_incidents: u32,
}

impl From<&SubsystemStatusEntryDto> for SubsystemHealthEntry {
    fn from(dto: &SubsystemStatusEntryDto) -> Self {
        Self {
            status: dto.effective_status,
            probe_status: dto.probe_status,
            mode: dto.mode,
            summary: dto.summary.clone(),
            updated_by: dto.updated_by.clone(),
            updated_at_ms: dto.updated_at_ms,
            auto_clear_at_ms: dto.auto_clear_at_ms,
            active_incidents: dto.active_incidents,
        }
    }
}

/// Typed 7-operational-subsystem health snapshot (`ci`, `cd`, `ab_deploy`, `repository`,
/// `observability`, `model_runner`, `platform`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubsystemHealthSummary {
    pub ci: SubsystemHealthEntry,
    pub cd: SubsystemHealthEntry,
    pub ab_deploy: SubsystemHealthEntry,
    pub repository: SubsystemHealthEntry,
    pub observability: SubsystemHealthEntry,
    pub model_runner: SubsystemHealthEntry,
    pub platform: SubsystemHealthEntry,
    #[serde(default)]
    pub updated_at_ms: u64,
}

impl SubsystemHealthSummary {
    /// Construct from a slice of [`SubsystemStatusEntryDto`] items.
    #[must_use]
    pub fn from_entries(entries: &[SubsystemStatusEntryDto]) -> Self {
        let mut summary = Self::default();
        for entry in entries {
            summary.apply_entry(entry);
        }
        summary
    }

    /// Update the matching operational subsystem entry in place.
    pub fn apply_entry(&mut self, entry: &SubsystemStatusEntryDto) {
        if let Some(slot) = self.get_mut(entry.subsystem) {
            *slot = SubsystemHealthEntry::from(entry);
            self.updated_at_ms = self.updated_at_ms.max(entry.updated_at_ms);
        }
    }

    /// Borrow health entry for an operational subsystem (`None` for [`SubsystemId::System`]).
    #[must_use]
    pub const fn get(&self, id: SubsystemId) -> Option<&SubsystemHealthEntry> {
        match id {
            SubsystemId::System => None,
            SubsystemId::Ci => Some(&self.ci),
            SubsystemId::Cd => Some(&self.cd),
            SubsystemId::AbDeploy => Some(&self.ab_deploy),
            SubsystemId::Repository => Some(&self.repository),
            SubsystemId::Observability => Some(&self.observability),
            SubsystemId::ModelRunner => Some(&self.model_runner),
            SubsystemId::Platform => Some(&self.platform),
        }
    }

    /// Mutably borrow health entry for an operational subsystem (`None` for [`SubsystemId::System`]).
    pub fn get_mut(&mut self, id: SubsystemId) -> Option<&mut SubsystemHealthEntry> {
        match id {
            SubsystemId::System => None,
            SubsystemId::Ci => Some(&mut self.ci),
            SubsystemId::Cd => Some(&mut self.cd),
            SubsystemId::AbDeploy => Some(&mut self.ab_deploy),
            SubsystemId::Repository => Some(&mut self.repository),
            SubsystemId::Observability => Some(&mut self.observability),
            SubsystemId::ModelRunner => Some(&mut self.model_runner),
            SubsystemId::Platform => Some(&mut self.platform),
        }
    }

    /// Return all 7 operational subsystem entries in canonical order.
    #[must_use]
    pub const fn entries(&self) -> [(SubsystemId, &SubsystemHealthEntry); 7] {
        [
            (SubsystemId::Ci, &self.ci),
            (SubsystemId::Cd, &self.cd),
            (SubsystemId::AbDeploy, &self.ab_deploy),
            (SubsystemId::Repository, &self.repository),
            (SubsystemId::Observability, &self.observability),
            (SubsystemId::ModelRunner, &self.model_runner),
            (SubsystemId::Platform, &self.platform),
        ]
    }

    /// Rolled-up platform banner status derived from the 7 operational subsystems.
    #[must_use]
    pub fn overall_status(&self) -> SubsystemStatus {
        let mut any_offline = false;
        let mut any_degraded = false;
        let mut any_partial_maint = false;
        let mut maint_count = 0usize;
        for (_, entry) in self.entries() {
            match entry.status {
                SubsystemStatus::Offline => any_offline = true,
                SubsystemStatus::Degraded => any_degraded = true,
                SubsystemStatus::InMaintenance => maint_count += 1,
                SubsystemStatus::PartialMaintenance => any_partial_maint = true,
                SubsystemStatus::Live => {}
            }
        }
        if any_offline {
            SubsystemStatus::Offline
        } else if any_degraded {
            SubsystemStatus::Degraded
        } else if maint_count == 7 {
            SubsystemStatus::InMaintenance
        } else if maint_count > 0 || any_partial_maint {
            SubsystemStatus::PartialMaintenance
        } else {
            SubsystemStatus::Live
        }
    }
}

/// SSE `event: subsystem_status_changed` on `/api/obs/v1/events`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubsystemStatusChangedDto {
    pub entry: SubsystemStatusEntryDto,
}

// ---------------------------------------------------------------- Ecosystem-Agnostic CD & A|B Rollback DTOs

/// Ecosystem identifier carried by [`CdRunDto`] (`"cargo"`, `"dotnet"`, `"maven"`, or custom).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CdEcosystem {
    #[default]
    Cargo,
    DotNet,
    Maven,
    Custom(String),
}

impl CdEcosystem {
    /// Normalize an ecosystem label cleanly (`".net"`/`"nuget"` -> [`Self::DotNet`]).
    #[must_use]
    pub fn from_label(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "cargo" | "rust" => Self::Cargo,
            "dotnet" | ".net" | "nuget" => Self::DotNet,
            "maven" | "java" => Self::Maven,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Canonical lowercase wire slug (`"cargo"`, `"dotnet"`, `"maven"`, or custom slug).
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cargo => "cargo",
            Self::DotNet => "dotnet",
            Self::Maven => "maven",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Human-facing badge label (`"Cargo"`, `".NET"`, `"Maven"`, or custom slug).
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Cargo => "Cargo",
            Self::DotNet => ".NET",
            Self::Maven => "Maven",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl Serialize for CdEcosystem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CdEcosystem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_label(&raw))
    }
}

/// Lifecycle state of an ecosystem-agnostic delivery run or step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdRunState {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Blocked,
    RolledBack,
}

/// One ordered delivery stage inside [`CdRunDto`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CdStepDto {
    pub step_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub label: String,
    #[serde(default)]
    pub state: CdRunState,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub skip_reason: Option<SkipReason>,
    #[serde(default)]
    pub error_summary: Option<String>,
    #[serde(default)]
    pub failed_package: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

/// Pinpointed failure diagnosis attached to a failed [`CdRunDto`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CdRunFailurePinpointDto {
    pub step_id: String,
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub package_version: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub reason: String,
    #[serde(default)]
    pub log_excerpt: Option<String>,
}

/// Ecosystem-agnostic `GET /api/cd/runs` release/publish record (`Area::Cd`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CdRunDto {
    pub run_id: String,
    #[serde(default)]
    pub ecosystem: CdEcosystem,
    #[serde(default)]
    pub repo: RepoId,
    #[serde(default, deserialize_with = "null_default")]
    pub ref_name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub commit_sha: String,
    #[serde(default)]
    pub state: CdRunState,
    #[serde(default)]
    pub started_epoch_ms: u64,
    #[serde(default)]
    pub finished_epoch_ms: Option<u64>,
    #[serde(default, deserialize_with = "null_default")]
    pub steps: Vec<CdStepDto>,
    #[serde(default)]
    pub failure: Option<CdRunFailurePinpointDto>,
    #[serde(default)]
    pub registry_package: Option<String>,
    #[serde(default)]
    pub registry_version: Option<String>,
    #[serde(default)]
    pub ab_deployment_id: Option<String>,
    #[serde(default)]
    pub artifact_digest: Option<String>,
}

/// `POST /api/cd/deployments/{id}/rollback` JSON body.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AbRollbackRequestDto {
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /api/cd/deployments/{id}/rollback` response.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AbRollbackResponseDto {
    pub deployment_id: String,
    #[serde(default)]
    pub restored_slot: Slot,
    #[serde(default)]
    pub tainted_digest: Option<String>,
}

// ---------------------------------------------------------------- Model-Runner DTOs

/// One resident model inside `GET /api/mr/v1/status`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResidentModelStatusDto {
    pub model_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub cluster: String,
    #[serde(default)]
    pub is_alive: bool,
    #[serde(default)]
    pub n_ctx: u32,
    #[serde(default)]
    pub mlock_bytes: u64,
    #[serde(default)]
    pub memlock_ceiling_bytes: u64,
    #[serde(default)]
    pub queue_depth: u32,
}

/// `GET /api/mr/v1/status` response (`folio:mr:read`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModelRunnerStatusDto {
    #[serde(default)]
    pub total_queue_depth: u32,
    #[serde(default)]
    pub is_executing: bool,
    #[serde(default)]
    pub estimated_wait_ms: u64,
    #[serde(default)]
    pub mlock_used_bytes: u64,
    #[serde(default)]
    pub memlock_ceiling_bytes: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub models: Vec<ResidentModelStatusDto>,
}

/// Sanitized WireGuard peer handshake summary inside [`ModelRunnerConnectionDto`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WireGuardPeerSummaryDto {
    pub public_key_prefix: String,
    #[serde(default, deserialize_with = "null_default")]
    pub allowed_ips: String,
    #[serde(default)]
    pub latest_handshake_epoch_ms: Option<u64>,
    #[serde(default)]
    pub rx_bytes: u64,
    #[serde(default)]
    pub tx_bytes: u64,
}

/// `GET /api/mr/v1/connection` response (`folio:mr:connect`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModelRunnerConnectionDto {
    #[serde(default, deserialize_with = "null_default")]
    pub openai_base_url: String,
    #[serde(default, deserialize_with = "null_default")]
    pub openai_routes: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub auth_header_guide: String,
    #[serde(default, deserialize_with = "null_default")]
    pub wireguard_endpoint: String,
    #[serde(default, deserialize_with = "null_default")]
    pub tunnel_cidr: String,
    #[serde(default, deserialize_with = "null_default")]
    pub interface_state: String,
    #[serde(default, deserialize_with = "null_default")]
    pub peers: Vec<WireGuardPeerSummaryDto>,
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

    use super::SubsystemId;

    /// CI event stream (existing endpoint, new event kinds).
    pub const CI_EVENTS: &str = "/api/events";
    /// CD event stream.
    pub const CD_EVENTS: &str = "/api/cd/events";
    /// Ecosystem-agnostic CD release/publish runs (`MuConDe-controller`).
    pub const CD_RUNS: &str = "/api/cd/runs";
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
    /// Signed-in user profile and synced skin/font preferences.
    pub const AUTH_PROFILE: &str = "/api/auth/profile";
    /// User Passkey credential list.
    pub const AUTH_PASSKEYS: &str = "/api/auth/passkeys";
    /// Begin WebAuthn Passkey enrollment ceremony.
    pub const AUTH_PASSKEYS_REGISTER_START: &str = "/api/auth/passkeys/register/start";
    /// Complete WebAuthn Passkey enrollment ceremony.
    pub const AUTH_PASSKEYS_REGISTER_FINISH: &str = "/api/auth/passkeys/register/finish";
    /// Begin Passkey step-up challenge for `Timed` role elevation.
    pub const AUTH_ELEVATE_START: &str = "/api/auth/elevate/start";
    /// Complete Passkey step-up verification for `Timed` role elevation.
    pub const AUTH_ELEVATE_FINISH: &str = "/api/auth/elevate/finish";
    /// Lock all active `Timed` elevated roles on the current session.
    pub const AUTH_ROLE_LOCK: &str = "/api/auth/roles/lock";
    /// Explicit user activity touch resetting active `Timed` `idle_timer`s.
    pub const AUTH_ACTIVITY: &str = "/api/auth/activity";
    /// Platform `RoleCatalog` management endpoint.
    pub const AUTH_ROLES: &str = "/api/auth/roles";
    /// Platform `PolicyFolio` catalog endpoint.
    pub const AUTH_FOLIOS: &str = "/api/auth/folios";
    /// Platform `RoleFolioIndex` endpoint.
    pub const AUTH_ROLE_FOLIO_INDEX: &str = "/api/auth/role-folio-index";
    /// Bound platform user directory endpoint.
    pub const AUTH_USERS: &str = "/api/auth/users";
    /// Platform onboarding invites endpoint.
    pub const AUTH_INVITES: &str = "/api/auth/invites";

    /// Registry package catalog (`MuRegist`).
    pub const REG_PACKAGES: &str = "/api/reg/v1/packages";
    /// Registry canary bundles & taint provenance (`MuRegist`).
    pub const REG_CANARIES: &str = "/api/reg/v1/canaries";
    /// Registry CAS storage health (`MuRegist`).
    pub const REG_CAS_STATUS: &str = "/api/reg/v1/cas/status";

    /// Model-Runner resident model & queue telemetry (`jero-manager`).
    pub const MR_STATUS: &str = "/api/mr/v1/status";
    /// Model-Runner OpenAI API & rootless WireGuard connection details (`jero-manager`).
    pub const MR_CONNECTION: &str = "/api/mr/v1/connection";

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
    /// Unified 7-subsystem operational health & maintenance feed (`MuObserve`).
    pub const OBS_SUBSYSTEM_STATUS: &str = "/api/obs/v1/status/subsystems";
    /// Observe ingress overview (`MuObserve`).
    pub const OBS_INGRESS_OVERVIEW: &str = "/api/obs/v1/ingress/overview";
    /// Observe dynamic ingress routes (`MuObserve`).
    pub const OBS_INGRESS_ROUTES: &str = "/api/obs/v1/ingress/routes";
    /// Observe ingress upstreams health (`MuObserve`).
    pub const OBS_INGRESS_UPSTREAMS: &str = "/api/obs/v1/ingress/upstreams";

    fn percent_encode_query(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        for b in input.bytes() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(b as char);
            } else {
                use core::fmt::Write;
                let _ = write!(out, "%{:02X}", b);
            }
        }
        out
    }

    /// `/api/obs/v1/logs?q={q}&limit={limit}` with RFC 3986 percent-encoding.
    #[must_use]
    pub fn obs_logs_query(q: &str, limit: usize) -> String {
        format!(
            "/api/obs/v1/logs?q={}&limit={}",
            percent_encode_query(q),
            limit
        )
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

    /// `/api/cd/runs?repo={owner}/{repo}&limit={limit}[&ecosystem={eco}]`.
    #[must_use]
    pub fn cd_runs(owner: &str, repo: &str, ecosystem: Option<&str>, limit: u32) -> String {
        let repo_param = format!("{owner}/{repo}");
        match ecosystem {
            Some(eco) if !eco.is_empty() => format!(
                "/api/cd/runs?repo={}&ecosystem={}&limit={}",
                percent_encode_query(&repo_param),
                percent_encode_query(eco),
                limit
            ),
            _ => format!(
                "/api/cd/runs?repo={}&limit={}",
                percent_encode_query(&repo_param),
                limit
            ),
        }
    }

    /// `/api/cd/deployments/{id}/gate`.
    #[must_use]
    pub fn gate(deployment_id: &str) -> String {
        format!("/api/cd/deployments/{deployment_id}/gate")
    }

    /// `/api/cd/deployments/{id}/rollback` (`POST` emergency symmetric Btrfs rollback).
    #[must_use]
    pub fn ab_rollback(deployment_id: &str) -> String {
        format!("/api/cd/deployments/{deployment_id}/rollback")
    }

    /// `/api/obs/v1/status/subsystems/{subsystem}/maintenance`.
    #[must_use]
    pub fn subsystem_maintenance(subsystem: SubsystemId) -> String {
        format!(
            "/api/obs/v1/status/subsystems/{}/maintenance",
            subsystem.as_str()
        )
    }

    /// `/api/auth/roles/{role_id}/enable`.
    #[must_use]
    pub fn role_enable(role_id: &str) -> String {
        format!("/api/auth/roles/{role_id}/enable")
    }

    /// `/api/auth/roles/{role_id}/disable`.
    #[must_use]
    pub fn role_disable(role_id: &str) -> String {
        format!("/api/auth/roles/{role_id}/disable")
    }

    /// `/api/auth/passkeys/{credential_id}`.
    #[must_use]
    pub fn auth_passkey(credential_id: &str) -> String {
        format!("/api/auth/passkeys/{credential_id}")
    }

    /// `/api/auth/users/{login}/roles`.
    #[must_use]
    pub fn auth_user_roles(login: &str) -> String {
        format!("/api/auth/users/{login}/roles")
    }

    /// `/api/auth/users/{login}/passkeys/reset`.
    #[must_use]
    pub fn auth_user_passkeys_reset(login: &str) -> String {
        format!("/api/auth/users/{login}/passkeys/reset")
    }

    /// `/api/auth/users/{login}`.
    #[must_use]
    pub fn auth_user(login: &str) -> String {
        format!("/api/auth/users/{login}")
    }

    /// `/api/auth/invites/{invite_id}`.
    #[must_use]
    pub fn auth_invite(invite_id: &str) -> String {
        format!("/api/auth/invites/{invite_id}")
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
