// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Messages exchanged by the UI module and the data worker.
//!
//! Identifiers and routes live here; the skin-agnostic view models carried
//! by [`DataEvent::Snapshot`] / [`DataEvent::Patch`] live in [`crate::view`].

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::config::WorkerConfig;
use crate::dto::{
    AuthSessionDto, MaintenanceMode, PolicyFolioDto, RoleActivationDto, RoleFolioIndexDto,
    RoleSpecDto, StepUpRequirement, SubsystemId, UserProfileDto,
};
use crate::view::{AreaBadges, DataSource, Delta, Slot, ViewModel};

/// Default UTF-8 character budget for the signed-in `+role[rem][idle]` header readout.
pub const DEFAULT_MAX_ACTIVE_ROLES_HEADER_CHARS: u32 = 120;

/// Top-level console area (8 canonical role-gated rail positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    #[default]
    #[serde(alias = "system", alias = "overview")]
    Dashboard,
    Ci,
    Cd,
    #[serde(alias = "ab-deploy", alias = "ab")]
    AbDeploy,
    #[serde(alias = "registry", alias = "reg")]
    Repository,
    #[serde(alias = "observe", alias = "obs")]
    Observability,
    #[serde(alias = "model-runner", alias = "mr")]
    ModelRunner,
    #[serde(alias = "iam")]
    Platform,
}

impl Area {
    /// All 8 canonical console areas in primary navigation order.
    pub const ALL: [Self; 8] = [
        Self::Dashboard,
        Self::Ci,
        Self::Cd,
        Self::AbDeploy,
        Self::Repository,
        Self::Observability,
        Self::ModelRunner,
        Self::Platform,
    ];

    /// Canonical URL route slug for this area.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::Ci => "ci",
            Self::Cd => "cd",
            Self::AbDeploy => "ab-deploy",
            Self::Repository => "repository",
            Self::Observability => "observability",
            Self::ModelRunner => "model-runner",
            Self::Platform => "platform",
        }
    }

    /// Full navigation label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Ci => "CI",
            Self::Cd => "CD",
            Self::AbDeploy => "A|B Deploy",
            Self::Repository => "Repository",
            Self::Observability => "Observability",
            Self::ModelRunner => "Model-Runner",
            Self::Platform => "Platform",
        }
    }

    /// Compact rail indicator label.
    #[must_use]
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Dashboard => "DASH",
            Self::Ci => "CI",
            Self::Cd => "CD",
            Self::AbDeploy => "A|B",
            Self::Repository => "REG",
            Self::Observability => "OBS",
            Self::ModelRunner => "MR",
            Self::Platform => "PLAT",
        }
    }

    /// Canonical `mu-auth` subsystem mapped 1:1 to this area.
    #[must_use]
    pub const fn subsystem(self) -> SubsystemId {
        match self {
            Self::Dashboard => SubsystemId::System,
            Self::Ci => SubsystemId::Ci,
            Self::Cd => SubsystemId::Cd,
            Self::AbDeploy => SubsystemId::AbDeploy,
            Self::Repository => SubsystemId::Repository,
            Self::Observability => SubsystemId::Observability,
            Self::ModelRunner => SubsystemId::ModelRunner,
            Self::Platform => SubsystemId::Platform,
        }
    }

    /// Parse a canonical slug or legacy route alias (`#/iam`, `#/registry`, `#/observe`, `#/fleet`).
    #[must_use]
    pub fn from_slug(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dashboard" | "system" | "overview" => Some(Self::Dashboard),
            "ci" | "fleet" => Some(Self::Ci),
            "cd" => Some(Self::Cd),
            "ab-deploy" | "ab_deploy" | "ab" => Some(Self::AbDeploy),
            "repository" | "registry" | "reg" => Some(Self::Repository),
            "observability" | "observe" | "obs" => Some(Self::Observability),
            "model-runner" | "model_runner" | "mr" => Some(Self::ModelRunner),
            "platform" | "iam" => Some(Self::Platform),
            _ => None,
        }
    }
}

/// Sub-views owned by [`Area::Dashboard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardSubView {
    #[default]
    Overview,
}

impl DashboardSubView {
    pub const ALL: [Self; 1] = [Self::Overview];
}

/// Sub-views owned by [`Area::Ci`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiSubView {
    #[default]
    Scorecards,
    Trainway,
    QueueLogs,
    WorkerNodes,
    SandboxProfiles,
}

impl CiSubView {
    pub const ALL: [Self; 5] = [
        Self::Scorecards,
        Self::Trainway,
        Self::QueueLogs,
        Self::WorkerNodes,
        Self::SandboxProfiles,
    ];
}

/// Sub-views owned by [`Area::Cd`] (ecosystem-agnostic release/publish pipelines).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdSubView {
    #[default]
    Runs,
    Pipelines,
}

impl CdSubView {
    pub const ALL: [Self; 2] = [Self::Runs, Self::Pipelines];
}

/// Sub-views owned by [`Area::AbDeploy`] (symmetric `@a`/`@b` deployment slots & gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbDeploySubView {
    #[default]
    IntegrationGate,
    DeploymentsSlots,
    GateTestLogs,
    ActiveLeases,
}

impl AbDeploySubView {
    pub const ALL: [Self; 4] = [
        Self::IntegrationGate,
        Self::DeploymentsSlots,
        Self::GateTestLogs,
        Self::ActiveLeases,
    ];
}

/// Sub-views owned by [`Area::Repository`] (`REG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositorySubView {
    #[default]
    Packages,
    #[serde(alias = "canaries_taints")]
    CanaryTaints,
    CasHealth,
}

impl RepositorySubView {
    /// Alias matching `CanariesTaints` naming.
    #[allow(non_upper_case_globals)]
    pub const CanariesTaints: Self = Self::CanaryTaints;

    pub const ALL: [Self; 3] = [Self::Packages, Self::CanaryTaints, Self::CasHealth];
}

/// Backwards-compatible alias for [`RepositorySubView`].
pub type RegistrySubView = RepositorySubView;

/// Sub-views owned by [`Area::Observability`] (`OBS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservabilitySubView {
    #[default]
    Smartscape,
    PurePath,
    ProblemsSlos,
    Logs,
    Metrics,
    DataStore,
    Ingress,
}

impl ObservabilitySubView {
    pub const ALL: [Self; 7] = [
        Self::Smartscape,
        Self::PurePath,
        Self::ProblemsSlos,
        Self::Logs,
        Self::Metrics,
        Self::DataStore,
        Self::Ingress,
    ];
}

/// Backwards-compatible alias for [`ObservabilitySubView`].
pub type ObserveSubView = ObservabilitySubView;

/// Sub-views owned by [`Area::ModelRunner`] (`MR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRunnerSubView {
    #[default]
    RuntimeQueue,
    ConnectionVpn,
}

impl ModelRunnerSubView {
    pub const ALL: [Self; 2] = [Self::RuntimeQueue, Self::ConnectionVpn];
}

/// Sub-views owned by [`Area::Platform`] (`PLAT`, replacing legacy IAM naming).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformSubView {
    #[default]
    Maintenance,
    UsersAndInvites,
    RolesAndFolios,
}

impl PlatformSubView {
    pub const ALL: [Self; 3] = [
        Self::Maintenance,
        Self::UsersAndInvites,
        Self::RolesAndFolios,
    ];
}

/// Active secondary sub-view across any of the 8 top-level areas (27 total).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubView {
    Dashboard(DashboardSubView),
    Ci(CiSubView),
    Cd(CdSubView),
    AbDeploy(AbDeploySubView),
    Repository(RepositorySubView),
    Observability(ObservabilitySubView),
    ModelRunner(ModelRunnerSubView),
    Platform(PlatformSubView),
}

impl Default for SubView {
    fn default() -> Self {
        Self::Dashboard(DashboardSubView::Overview)
    }
}

impl SubView {
    /// Default sub-view when switching into `area`.
    #[must_use]
    pub const fn default_for(area: Area) -> Self {
        match area {
            Area::Dashboard => Self::Dashboard(DashboardSubView::Overview),
            Area::Ci => Self::Ci(CiSubView::Scorecards),
            Area::Cd => Self::Cd(CdSubView::Runs),
            Area::AbDeploy => Self::AbDeploy(AbDeploySubView::IntegrationGate),
            Area::Repository => Self::Repository(RepositorySubView::Packages),
            Area::Observability => Self::Observability(ObservabilitySubView::Smartscape),
            Area::ModelRunner => Self::ModelRunner(ModelRunnerSubView::RuntimeQueue),
            Area::Platform => Self::Platform(PlatformSubView::Maintenance),
        }
    }

    /// Parent [`Area`] of this sub-view.
    #[must_use]
    pub const fn area(self) -> Area {
        match self {
            Self::Dashboard(_) => Area::Dashboard,
            Self::Ci(_) => Area::Ci,
            Self::Cd(_) => Area::Cd,
            Self::AbDeploy(_) => Area::AbDeploy,
            Self::Repository(_) => Area::Repository,
            Self::Observability(_) => Area::Observability,
            Self::ModelRunner(_) => Area::ModelRunner,
            Self::Platform(_) => Area::Platform,
        }
    }

    /// Canonical URL slug for this sub-view (`overview`, `scorecards`, `runs`, `gate`, etc.).
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Dashboard(DashboardSubView::Overview) => "overview",
            Self::Ci(CiSubView::Scorecards) => "scorecards",
            Self::Ci(CiSubView::Trainway) => "trainway",
            Self::Ci(CiSubView::QueueLogs) => "logs",
            Self::Ci(CiSubView::WorkerNodes) => "nodes",
            Self::Ci(CiSubView::SandboxProfiles) => "sandboxes",
            Self::Cd(CdSubView::Runs) => "runs",
            Self::Cd(CdSubView::Pipelines) => "pipelines",
            Self::AbDeploy(AbDeploySubView::IntegrationGate) => "gate",
            Self::AbDeploy(AbDeploySubView::DeploymentsSlots) => "deployments",
            Self::AbDeploy(AbDeploySubView::GateTestLogs) => "gate-logs",
            Self::AbDeploy(AbDeploySubView::ActiveLeases) => "leases",
            Self::Repository(RepositorySubView::Packages) => "packages",
            Self::Repository(RepositorySubView::CanaryTaints) => "canaries",
            Self::Repository(RepositorySubView::CasHealth) => "cas",
            Self::Observability(ObservabilitySubView::Smartscape) => "smartscape",
            Self::Observability(ObservabilitySubView::PurePath) => "purepath",
            Self::Observability(ObservabilitySubView::ProblemsSlos) => "problems",
            Self::Observability(ObservabilitySubView::Logs) => "logs",
            Self::Observability(ObservabilitySubView::Metrics) => "metrics",
            Self::Observability(ObservabilitySubView::DataStore) => "store",
            Self::Observability(ObservabilitySubView::Ingress) => "ingress",
            Self::ModelRunner(ModelRunnerSubView::RuntimeQueue) => "runtime",
            Self::ModelRunner(ModelRunnerSubView::ConnectionVpn) => "connection",
            Self::Platform(PlatformSubView::Maintenance) => "maintenance",
            Self::Platform(PlatformSubView::UsersAndInvites) => "users",
            Self::Platform(PlatformSubView::RolesAndFolios) => "roles",
        }
    }

    /// Display label on the sub-view bar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dashboard(DashboardSubView::Overview) => "Overview",
            Self::Ci(CiSubView::Scorecards) => "Scorecards",
            Self::Ci(CiSubView::Trainway) => "Trainway",
            Self::Ci(CiSubView::QueueLogs) => "Queue & Logs",
            Self::Ci(CiSubView::WorkerNodes) => "Worker Nodes",
            Self::Ci(CiSubView::SandboxProfiles) => "Sandbox Profiles",
            Self::Cd(CdSubView::Runs) => "Recent Runs",
            Self::Cd(CdSubView::Pipelines) => "Delivery Pipelines",
            Self::AbDeploy(AbDeploySubView::IntegrationGate) => "Integration Gate",
            Self::AbDeploy(AbDeploySubView::DeploymentsSlots) => "Deployments & Slots",
            Self::AbDeploy(AbDeploySubView::GateTestLogs) => "Gate Test Logs",
            Self::AbDeploy(AbDeploySubView::ActiveLeases) => "Active Leases",
            Self::Repository(RepositorySubView::Packages) => "Packages",
            Self::Repository(RepositorySubView::CanaryTaints) => "Canary Bundles & Taints",
            Self::Repository(RepositorySubView::CasHealth) => "CAS Storage Health",
            Self::Observability(ObservabilitySubView::Smartscape) => "Smartscape",
            Self::Observability(ObservabilitySubView::PurePath) => "PurePath",
            Self::Observability(ObservabilitySubView::ProblemsSlos) => "Problems & SLOs",
            Self::Observability(ObservabilitySubView::Logs) => "Logs",
            Self::Observability(ObservabilitySubView::Metrics) => "Metrics",
            Self::Observability(ObservabilitySubView::DataStore) => "Data Store",
            Self::Observability(ObservabilitySubView::Ingress) => "Ingress & Edge",
            Self::ModelRunner(ModelRunnerSubView::RuntimeQueue) => "Runtime & Queue",
            Self::ModelRunner(ModelRunnerSubView::ConnectionVpn) => "Connection & VPN",
            Self::Platform(PlatformSubView::Maintenance) => "Maintenance",
            Self::Platform(PlatformSubView::UsersAndInvites) => "Users & Invites",
            Self::Platform(PlatformSubView::RolesAndFolios) => "Roles & Folios",
        }
    }

    /// Ordered sub-views for `area`.
    #[must_use]
    pub const fn all_for(area: Area) -> &'static [Self] {
        match area {
            Area::Dashboard => &[Self::Dashboard(DashboardSubView::Overview)],
            Area::Ci => &[
                Self::Ci(CiSubView::Scorecards),
                Self::Ci(CiSubView::Trainway),
                Self::Ci(CiSubView::QueueLogs),
                Self::Ci(CiSubView::WorkerNodes),
                Self::Ci(CiSubView::SandboxProfiles),
            ],
            Area::Cd => &[Self::Cd(CdSubView::Runs), Self::Cd(CdSubView::Pipelines)],
            Area::AbDeploy => &[
                Self::AbDeploy(AbDeploySubView::IntegrationGate),
                Self::AbDeploy(AbDeploySubView::DeploymentsSlots),
                Self::AbDeploy(AbDeploySubView::GateTestLogs),
                Self::AbDeploy(AbDeploySubView::ActiveLeases),
            ],
            Area::Repository => &[
                Self::Repository(RepositorySubView::Packages),
                Self::Repository(RepositorySubView::CanaryTaints),
                Self::Repository(RepositorySubView::CasHealth),
            ],
            Area::Observability => &[
                Self::Observability(ObservabilitySubView::Smartscape),
                Self::Observability(ObservabilitySubView::PurePath),
                Self::Observability(ObservabilitySubView::ProblemsSlos),
                Self::Observability(ObservabilitySubView::Logs),
                Self::Observability(ObservabilitySubView::Metrics),
                Self::Observability(ObservabilitySubView::DataStore),
                Self::Observability(ObservabilitySubView::Ingress),
            ],
            Area::ModelRunner => &[
                Self::ModelRunner(ModelRunnerSubView::RuntimeQueue),
                Self::ModelRunner(ModelRunnerSubView::ConnectionVpn),
            ],
            Area::Platform => &[
                Self::Platform(PlatformSubView::Maintenance),
                Self::Platform(PlatformSubView::UsersAndInvites),
                Self::Platform(PlatformSubView::RolesAndFolios),
            ],
        }
    }

    /// Resolve a sub-view slug inside `area` (accepting M2 canonical and legacy sub-view slugs).
    #[must_use]
    pub fn from_area_and_slug(area: Area, raw_sub: &str) -> Option<Self> {
        let slug = raw_sub.trim().to_ascii_lowercase();
        match area {
            Area::Dashboard => match slug.as_str() {
                "" | "overview" | "health" => Some(Self::Dashboard(DashboardSubView::Overview)),
                _ => None,
            },
            Area::Ci => match slug.as_str() {
                "" | "scorecards" => Some(Self::Ci(CiSubView::Scorecards)),
                "trainway" => Some(Self::Ci(CiSubView::Trainway)),
                "logs" | "queue" | "queue_logs" | "queue-logs" => {
                    Some(Self::Ci(CiSubView::QueueLogs))
                }
                "nodes" | "worker-nodes" | "worker_nodes" => Some(Self::Ci(CiSubView::WorkerNodes)),
                "sandboxes" | "sandbox-profiles" | "sandbox_profiles" => {
                    Some(Self::Ci(CiSubView::SandboxProfiles))
                }
                _ => None,
            },
            Area::Cd => match slug.as_str() {
                "" | "runs" => Some(Self::Cd(CdSubView::Runs)),
                "pipelines" => Some(Self::Cd(CdSubView::Pipelines)),
                // Legacy v5 CD subviews promoted cleanly to Area::AbDeploy:
                "gate" => Some(Self::AbDeploy(AbDeploySubView::IntegrationGate)),
                "deployments" | "slots" => Some(Self::AbDeploy(AbDeploySubView::DeploymentsSlots)),
                "gate-logs" | "gate_logs" => Some(Self::AbDeploy(AbDeploySubView::GateTestLogs)),
                "leases" => Some(Self::AbDeploy(AbDeploySubView::ActiveLeases)),
                _ => None,
            },
            Area::AbDeploy => match slug.as_str() {
                "" | "gate" | "integration-gate" => {
                    Some(Self::AbDeploy(AbDeploySubView::IntegrationGate))
                }
                "deployments" | "slots" => Some(Self::AbDeploy(AbDeploySubView::DeploymentsSlots)),
                "gate-logs" | "gate_logs" | "logs" => {
                    Some(Self::AbDeploy(AbDeploySubView::GateTestLogs))
                }
                "leases" | "active-leases" => Some(Self::AbDeploy(AbDeploySubView::ActiveLeases)),
                _ => None,
            },
            Area::Repository => match slug.as_str() {
                "" | "packages" => Some(Self::Repository(RepositorySubView::Packages)),
                "canaries" | "canaries_taints" | "taints" => {
                    Some(Self::Repository(RepositorySubView::CanaryTaints))
                }
                "cas" | "cas_health" => Some(Self::Repository(RepositorySubView::CasHealth)),
                _ => None,
            },
            Area::Observability => match slug.as_str() {
                "" | "smartscape" => Some(Self::Observability(ObservabilitySubView::Smartscape)),
                "purepath" => Some(Self::Observability(ObservabilitySubView::PurePath)),
                "problems" | "slos" => {
                    Some(Self::Observability(ObservabilitySubView::ProblemsSlos))
                }
                "logs" => Some(Self::Observability(ObservabilitySubView::Logs)),
                "metrics" => Some(Self::Observability(ObservabilitySubView::Metrics)),
                "store" | "data-store" => {
                    Some(Self::Observability(ObservabilitySubView::DataStore))
                }
                "ingress" => Some(Self::Observability(ObservabilitySubView::Ingress)),
                _ => None,
            },
            Area::ModelRunner => match slug.as_str() {
                "" | "runtime" | "queue" => {
                    Some(Self::ModelRunner(ModelRunnerSubView::RuntimeQueue))
                }
                "connection" | "vpn" => Some(Self::ModelRunner(ModelRunnerSubView::ConnectionVpn)),
                _ => None,
            },
            Area::Platform => match slug.as_str() {
                "" | "maintenance" => Some(Self::Platform(PlatformSubView::Maintenance)),
                "users" | "invites" | "users-and-invites" | "users_and_invites" => {
                    Some(Self::Platform(PlatformSubView::UsersAndInvites))
                }
                "roles" | "folios" | "roles-and-folios" | "roles_and_folios" => {
                    Some(Self::Platform(PlatformSubView::RolesAndFolios))
                }
                _ => None,
            },
        }
    }

    /// Cycle within the same [`Area`] by `delta` tabs.
    #[must_use]
    pub fn step(self, delta: isize) -> Self {
        let list = Self::all_for(self.area());
        let idx = list.iter().position(|x| *x == self).unwrap_or(0);
        let len = isize::try_from(list.len()).unwrap_or(1);
        let cur = isize::try_from(idx).unwrap_or(0);
        let next = usize::try_from((cur + delta).rem_euclid(len)).unwrap_or(0);
        list[next]
    }
}

/// `owner/repo` pair identifying a repository.
///
/// Ordering is `owner`, then `repo`; view-model repository lists are kept in
/// this order so diffs and patches are deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct RepoId {
    pub owner: String,
    pub repo: String,
}

impl RepoId {
    /// Build a repository id from its two halves.
    #[must_use]
    pub fn new(owner: &str, repo: &str) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
        }
    }
}

/// Submodule of a superproject, identified by its path in `.gitmodules`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SubmoduleId {
    pub path: String,
}

/// Hash route: `#/{area}/{subview}?repo={owner}/{repo}&trace={trace_id}&filter={pill}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub area: Area,
    #[serde(default)]
    pub subview: SubView,
    pub repo: Option<RepoId>,
    pub submodule: Option<SubmoduleId>,
    pub branch: Option<String>,
    pub job: Option<u64>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            area: Area::Dashboard,
            subview: SubView::Dashboard(DashboardSubView::Overview),
            repo: None,
            submodule: None,
            branch: None,
            job: None,
            trace_id: None,
            filter: None,
        }
    }
}

impl Route {
    /// Construct the canonical default route for `area`.
    #[must_use]
    pub fn for_area(area: Area) -> Self {
        Self {
            area,
            subview: SubView::default_for(area),
            ..Self::default()
        }
    }

    /// Construct a route aligned with `subview.area()`.
    #[must_use]
    pub fn for_subview(subview: SubView) -> Self {
        Self {
            area: subview.area(),
            subview,
            ..Self::default()
        }
    }

    /// Format this route as a URL fragment (`#/{area}/{subview}[?...]`).
    #[must_use]
    pub fn to_hash(&self) -> String {
        let mut out = format!("#/{}/{}", self.area.slug(), self.subview.slug());
        let mut sep = '?';
        if let Some(repo) = &self.repo {
            out.push(sep);
            out.push_str(&format!("repo={}/{}", repo.owner, repo.repo));
            sep = '&';
        }
        if let Some(sub) = &self.submodule {
            out.push(sep);
            out.push_str(&format!("submodule={}", sub.path));
            sep = '&';
        }
        if let Some(branch) = &self.branch {
            out.push(sep);
            out.push_str(&format!("branch={branch}"));
            sep = '&';
        }
        if let Some(job) = self.job {
            out.push(sep);
            out.push_str(&format!("job={job}"));
            sep = '&';
        }
        if let Some(trace) = &self.trace_id {
            out.push(sep);
            out.push_str(&format!("trace={trace}"));
            sep = '&';
        }
        if let Some(filter) = &self.filter {
            out.push(sep);
            out.push_str(&format!("filter={filter}"));
        }
        out
    }

    /// Parse a browser hash fragment (`#/{area}/{subview}?...`), mapping legacy aliases (`#/iam`,
    /// `#/fleet`, `#/registry`, `#/observe`, `#/cd/gate`) to canonical v6 routes.
    #[must_use]
    pub fn try_from_hash(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let no_hash = trimmed.strip_prefix('#').unwrap_or(trimmed);
        let (path_part, query_part) = match no_hash.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (no_hash, None),
        };
        let path = path_part.trim_start_matches('/').trim_end_matches('/');
        if path.is_empty() {
            return None;
        }
        let mut segs = path.split('/');
        let raw_area = segs.next()?;
        let area_hint = Area::from_slug(raw_area)?;
        let raw_sub = segs.next().unwrap_or("");
        let is_legacy_fleet = raw_area.eq_ignore_ascii_case("fleet");
        let subview = if is_legacy_fleet && raw_sub.is_empty() {
            SubView::Ci(CiSubView::WorkerNodes)
        } else if raw_sub.is_empty() {
            SubView::default_for(area_hint)
        } else {
            SubView::from_area_and_slug(area_hint, raw_sub)?
        };
        let mut route = Self::for_subview(subview);
        if let Some(q) = query_part {
            for pair in q.split('&').filter(|p| !p.is_empty()) {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                match k {
                    "repo" => {
                        if let Some((owner, repo)) = v.split_once('/') {
                            if !owner.is_empty() && !repo.is_empty() {
                                route.repo = Some(RepoId::new(owner, repo));
                            }
                        }
                    }
                    "submodule" if !v.is_empty() => {
                        route.submodule = Some(SubmoduleId { path: v.into() });
                    }
                    "branch" if !v.is_empty() => route.branch = Some(v.into()),
                    "job" => route.job = v.parse::<u64>().ok(),
                    "trace" if !v.is_empty() => route.trace_id = Some(v.into()),
                    "filter" if !v.is_empty() => route.filter = Some(v.into()),
                    _ => {}
                }
            }
        }
        Some(route)
    }

    /// Parse a hash fragment, falling back to [`Route::default`] on empty or unknown input.
    #[must_use]
    pub fn from_hash(raw: &str) -> Self {
        Self::try_from_hash(raw).unwrap_or_default()
    }
}

/// Terminal / console font family. The layout solver re-measures on change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CrtFont {
    /// Hurmit Nerd Font Mono (default).
    #[default]
    Hurmit,
    /// OpenDyslexicM Nerd Font Mono (accessibility switch).
    OpenDyslexic,
}

/// Visual skin (`Html` modern semantic CSS/HTML5 default; `Wireframe` high-contrast vector).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleSkin {
    /// Semantic HTML5 & responsive natural-scroll CSS layout (default).
    #[default]
    Html,
    /// Flat high-contrast monochrome wireframe projection.
    Wireframe,
}

impl ConsoleSkin {
    /// Both supported v6 console skins (`Avionics` purged in v6).
    pub const ALL: [Self; 2] = [Self::Html, Self::Wireframe];

    /// Machine skin identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Wireframe => "wireframe",
        }
    }

    /// Human-readable label for profile preference selectors.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Wireframe => "Wireframe",
        }
    }
}

/// Panels that support panel-local paging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelId {
    Dashboard,
    Carousel,
    Trainway,
    Delivery,
    AbDeploy,
    Repository,
    Observability,
    ModelRunner,
    Platform,
    CiNodes,
    RepoPicker,
    Log,
}

/// Structured machine-readable reason for skipped CI or CD stages (`UI` localizes without wire prose).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Zero live capable worker nodes satisfy the requested hardware capability (e.g. `kva23`).
    NoCapableWorkerNode {
        #[serde(default)]
        capability: Option<String>,
    },
    /// Upstream dependency stage failed or was skipped.
    UpstreamStageNotPassed {
        #[serde(default)]
        stage_id: Option<String>,
    },
    /// Workflow filter or condition excluded this stage.
    ConditionNotMet,
    /// Superseded by a newer commit or operator cancellation.
    Superseded,
    /// Extension or unclassified structured code token.
    Other(String),
}

/// Structured machine-readable authorization/authentication status code (zero invite disclosure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthReasonCode {
    /// Caller is unauthenticated and requested a protected route or action.
    AuthenticationRequired,
    /// Authenticated identity is not authorized (fail-closed, zero invite disclosure).
    AuthorizationDenied,
    /// Bound operator must enroll at least one Passkey before console baseline activates.
    PasskeyEnrollmentRequired,
    /// Requested Timed or step-up role activation requires a fresh Passkey WebAuthn assertion.
    StepUpRequired,
    /// Active Timed lease reached hard or idle `00:00` expiry.
    LeaseExpired,
    /// Target subsystem or action is disabled by operator maintenance override or policy.
    SubsystemDisabled,
    /// Canonical `PinnedBaseline` role cannot be deactivated.
    PinnedBaselineImmutable,
    /// Target host TTL ceiling or quota guard rejected request.
    CeilingExceeded,
}

/// Active lease status view tracked per `Timed` elevated role (`[seconds_remaining][idle_timer]`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoleLeaseView {
    /// Elevated `Timed` role identifier.
    pub role_id: String,
    /// Optional resolved display name (falls back to `role_id` when unset).
    #[serde(default)]
    pub display_name: Option<String>,
    /// Lease activation timestamp (`epoch_ms`).
    #[serde(default)]
    pub activated_at_ms: u64,
    /// Most recent explicit user activity timestamp (`epoch_ms`).
    #[serde(default)]
    pub last_activity_at_ms: u64,
    /// Non-renewable hard expiry timestamp (`epoch_ms`).
    #[serde(default)]
    pub hard_expires_at_ms: u64,
    /// Idle expiry timestamp (`epoch_ms`), capped by `hard_expires_at_ms`.
    #[serde(default)]
    pub idle_expires_at_ms: u64,
    /// Server-enforced hard TTL ceiling in seconds.
    #[serde(default)]
    pub hard_ttl_secs: u64,
    /// Server-enforced idle timeout in seconds.
    #[serde(default)]
    pub idle_ttl_secs: u64,
    /// Whole seconds remaining until non-renewable hard expiry (`[seconds_remaining]`).
    #[serde(default)]
    pub seconds_remaining: u64,
    /// Whole seconds remaining until idle timeout (`[idle_timer]`).
    #[serde(default)]
    pub idle_timer: u64,
}

impl RoleLeaseView {
    /// Format a whole-second countdown as `MM:SS`.
    #[must_use]
    pub fn format_mm_ss(secs: u64) -> String {
        let mins = secs / 60;
        let rem = secs % 60;
        format!("{mins:02}:{rem:02}")
    }

    /// Recompute `seconds_remaining` and `idle_timer` at `now_ms`, returning `true` while active.
    pub fn tick_at(&mut self, now_ms: u64) -> bool {
        self.seconds_remaining = self
            .hard_expires_at_ms
            .saturating_sub(now_ms)
            .div_ceil(1_000);
        let effective_idle_ms = self.idle_expires_at_ms.min(self.hard_expires_at_ms);
        self.idle_timer = effective_idle_ms.saturating_sub(now_ms).div_ceil(1_000);
        !self.is_expired()
    }

    /// Return `true` when either hard or idle timer reached `0`.
    #[must_use]
    pub const fn is_expired(&self) -> bool {
        self.seconds_remaining == 0 || self.idle_timer == 0
    }
}

/// Active tab inside [`DialogModel::UserProfile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserProfileTab {
    #[default]
    UserDetails,
    Preferences,
    Passkeys,
}

/// One dormant or active role option rendered inside [`DialogModel::SessionRoles`].
///
/// Intentionally carries zero user-editable TTL fields; `Timed` TTL bounds are server-fixed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRoleDescriptor {
    pub role_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: String,
    pub activation: RoleActivationDto,
    pub step_up: StepUpRequirement,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub lease: Option<RoleLeaseView>,
    #[serde(default)]
    pub step_up_verified: bool,
}

/// Target action guarded by [`DialogModel::RiskConfirm`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskConfirmAction {
    RestartCiJob {
        job_id: u64,
        job_id_str: String,
        repo: RepoId,
    },
    TriggerAbRollback {
        deployment_id: String,
        repo: RepoId,
        target_node: String,
        current_slot: Slot,
        restore_slot: Slot,
        #[serde(default)]
        note: Option<String>,
    },
    SetSubsystemMaintenance {
        subsystem: SubsystemId,
        mode: MaintenanceMode,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        auto_clear_secs: Option<u64>,
    },
    UpdateUserRoles {
        github_login: String,
        bound_roles: Vec<String>,
    },
    ResetUserPasskeys {
        github_login: String,
    },
    RevokeUser {
        github_login: String,
    },
    UpsertRoleSpec {
        role_id: String,
    },
    DeleteRoleSpec {
        role_id: String,
    },
    UpsertPolicyFolio {
        folio_id: String,
    },
    DeletePolicyFolio {
        folio_id: String,
    },
    UpdateRoleFolioIndex,
}

/// Blocking mandatory post-login Passkey enrollment gate payload.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PasskeyEnrollmentGateModel {
    pub github_login: String,
    #[serde(default)]
    pub passkey_count: u32,
    pub reason_code: Option<AuthReasonCode>,
}

/// Signed-in operator `@user` modal payload (`User Details | Preferences | Passkeys`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserProfileDialogModel {
    pub profile: UserProfileDto,
    #[serde(default)]
    pub active_tab: UserProfileTab,
    #[serde(default)]
    pub max_session_expires_epoch_ms: Option<u64>,
}

/// Signed-in `+role` session role toggle and Timed Passkey step-up modal payload.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionRolesDialogModel {
    pub github_login: String,
    #[serde(default)]
    pub roles: Vec<SessionRoleDescriptor>,
    #[serde(default)]
    pub lock_all_enabled: bool,
}

/// Explicit risk-confirmation modal payload replacing hold-to-arm switches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskConfirmDialogModel {
    pub action: RiskConfirmAction,
    #[serde(default)]
    pub confirmation_phrase: Option<String>,
    #[serde(default)]
    pub typed_confirmation: String,
}

/// Single-display onboarding invite secret modal payload (`Platform > Users & Invites`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InviteCreatedOnceDialogModel {
    pub invite_id: String,
    pub invite_url: String,
    pub granted_role: String,
    pub expires_at_ms: u64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Skin-agnostic modal descriptor shared across [`ConsoleSkin::Html`] (`<dialog>`)
/// and [`ConsoleSkin::Wireframe`] (modal overlay frame).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogModel {
    /// Non-dismissible mandatory Passkey gate whenever `passkey_count == 0`.
    PasskeyEnrollmentGate(PasskeyEnrollmentGateModel),
    /// `@user` profile, preferences (`Html`/`Wireframe`, font), and Passkeys modal.
    UserProfile(UserProfileDialogModel),
    /// `+role` modal with 1-click Elective toggles, Passkey Timed elevation, and Lock Timed Roles.
    SessionRoles(SessionRolesDialogModel),
    /// Explicit confirmation modal for destructive/stateful operations.
    RiskConfirm(RiskConfirmDialogModel),
    /// One-time onboarding invite handoff modal (shown once upon creation).
    InviteCreatedOnce(InviteCreatedOnceDialogModel),
}

impl DialogModel {
    /// Canonical machine dialog kind slug (`passkey-enrollment-gate`, etc.).
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::PasskeyEnrollmentGate(_) => "passkey-enrollment-gate",
            Self::UserProfile(_) => "user-profile",
            Self::SessionRoles(_) => "session-roles",
            Self::RiskConfirm(_) => "risk-confirm",
            Self::InviteCreatedOnce(_) => "invite-created-once",
        }
    }

    /// `false` for blocking [`DialogModel::PasskeyEnrollmentGate`]; `true` for all dismissible modals.
    #[must_use]
    pub const fn is_dismissible(&self) -> bool {
        !matches!(self, Self::PasskeyEnrollmentGate(_))
    }
}

/// UI -> data worker.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiCommand {
    /// First command after spawning the worker: API base URL and optional
    /// fixture scenario, both parsed from the page query string.
    Configure(WorkerConfig),
    Subscribe(Route),
    /// Switch active sub-view inside the current or target area.
    SelectSubView(SubView),
    /// Applied after the repo/submodule selector settles (350 ms after the last step).
    SelectRepo {
        repo: RepoId,
        submodule: Option<SubmoduleId>,
    },
    Page {
        panel: PanelId,
        cursor: u32,
    },
    /// Reconnect immediately (live) or replay from the start (scenario).
    Retry,
    /// Client-side SSE stream reset & snapshot re-fetch triggered by Trainway restart.
    ResetStream,
    /// Tab hidden: the worker throttles to heartbeat-only.
    SetVisibility {
        hidden: bool,
    },
    SetCrtFont(CrtFont),
    /// Switch active visual skin (`Html` or `Wireframe`).
    SetSkin(ConsoleSkin),
    /// Redeem a one-time onboarding invite token (`POST /api/auth/redeem`).
    RedeemInvite {
        token: String,
    },
    /// Re-fetch `/api/auth/session` and synchronize user capabilities.
    RefreshSession,
    /// Directly set the operator authentication session state (tests & offline scenarios).
    SetAuthSession(AuthSessionDto),
    /// Enable an `Elective` (or server-default `PinnedBaseline`) role (zero client TTL parameters).
    EnableRole {
        role_id: String,
    },
    /// Disable an `Elective` or `Timed` role (`PinnedBaseline` cannot be deactivated).
    DisableRole {
        role_id: String,
    },
    /// Activate a `Timed` (or step-up `Elective`) role using a verified Passkey assertion (zero client TTL parameters).
    ElevateWithPasskey {
        role_id: String,
        assertion_json: String,
    },
    /// Immediately revoke all active `Timed` leases on this session.
    LockTimedRoles,
    /// Forward explicit user pointer/keyboard activity to reset active `Timed` `idle_timer`s.
    RecordUserActivity,
    /// Update editable `@user` display name and/or synced skin & font preferences.
    UpdateUserProfile {
        display_name: Option<String>,
        preferred_skin: Option<ConsoleSkin>,
        preferred_font: Option<CrtFont>,
    },
    /// Enroll a new WebAuthn Passkey credential on the signed-in account.
    EnrollPasskey {
        label: Option<String>,
        attestation_json: String,
    },
    /// Remove an enrolled Passkey after step-up Passkey verification.
    RemovePasskey {
        credential_id: String,
        step_up_assertion_json: String,
    },
    /// Open or switch active skin-agnostic modal dialog.
    OpenDialog(DialogModel),
    /// Close active dismissible modal dialog.
    CloseDialog,
    /// Restart / retry a specific CI job (`POST /api/jobs/{id}/restart`).
    RestartJob {
        job_id: u64,
    },
    /// Resume an interrupted/failed CI job (`POST /api/jobs/{id}/resume`, preserving completed stages).
    ResumeJob {
        job_id: u64,
    },
    /// Select a CI job in `CI > Queue & Logs` and fetch its detail/log (`GET /api/jobs/{id}` and `/api/jobs/{id}/log`).
    SelectJob {
        job_id: u64,
    },
    /// Trigger emergency symmetric A|B slot rollback (`POST /api/cd/deployments/{id}/rollback`).
    ///
    /// Note: manual A|B promotion is intentionally omitted; promotion stays 100% automated.
    TriggerAbRollback {
        deployment_id: String,
        #[serde(default)]
        note: Option<String>,
    },
    /// Set or clear an operator maintenance override (`PUT /api/obs/v1/status/subsystems/{id}/maintenance`).
    SetSubsystemMaintenance {
        subsystem: SubsystemId,
        mode: MaintenanceMode,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        auto_clear_secs: Option<u64>,
    },
    /// Mint a single-`PinnedBaseline` onboarding invite token (`POST /api/auth/invites`).
    CreateInvite {
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        ttl_secs: Option<u64>,
    },
    /// Revoke an outstanding onboarding invite (`DELETE /api/auth/invites/{id}`).
    RevokeInvite {
        invite_id: String,
    },
    /// Update dormant role bindings for an existing user (`PUT /api/auth/users/{login}/roles`).
    UpdateUserRoles {
        github_login: String,
        bound_roles: Vec<String>,
    },
    /// Reset a user's Passkeys back to `0` (`POST /api/auth/users/{login}/passkeys/reset`).
    ResetUserPasskeys {
        github_login: String,
    },
    /// Remove a user's bindings (`DELETE /api/auth/users/{login}`).
    RevokeUser {
        github_login: String,
    },
    /// Create or update a `RoleCatalog` role specification (`PUT /api/auth/roles`).
    UpsertRoleSpec(RoleSpecDto),
    /// Remove a non-baseline role specification (`DELETE /api/auth/roles/{id}`).
    DeleteRoleSpec {
        role_id: String,
    },
    /// Create or update a mutable Operator composite [`PolicyFolioDto`] (`PUT /api/auth/folios`).
    UpsertPolicyFolio(PolicyFolioDto),
    /// Delete an unreferenced mutable Operator [`PolicyFolioDto`] (`DELETE /api/auth/folios/{id}`).
    DeletePolicyFolio {
        folio_id: String,
    },
    /// Update [`RoleFolioIndexDto`] (`anonymous` and per-role folio grants, `PUT /api/auth/role-folio-index`).
    UpdateRoleFolioIndex(RoleFolioIndexDto),
    /// Client-side or browser navigation requested by the UI.
    Navigate {
        href: String,
    },
}

/// Connection state of the worker's upstream streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnState {
    Connecting,
    Live,
    /// A stream dropped; the next attempt starts in `retry_in_ms`.
    Reconnecting {
        attempt: u32,
        retry_in_ms: u32,
    },
    /// No upstream; the view comes from the offline snapshot (if any).
    Offline,
    /// Tab hidden: streams stay open but the UI only gets heartbeats.
    Paused,
    /// A fixture scenario is replaying instead of live data.
    Scenario,
}

/// Errors surfaced to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiError {
    /// Frame carried a different protocol version: a reload is required.
    VersionMismatch {
        expected: u32,
        found: u32,
    },
    /// Structured authentication/authorization denial without leaking internal wording.
    AuthDenied(AuthReasonCode),
    Http {
        status: u32,
        message: String,
    },
    Decode(String),
    /// Upstream SSE stream failure (network error, bad status).
    Stream(String),
    /// `?scenario=` named an unknown fixture; the worker fell back to live.
    UnknownScenario(String),
}

/// Data worker -> UI.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataEvent {
    /// Sent once after [`UiCommand::Configure`].
    Ready {
        protocol: u32,
        source: DataSource,
    },
    Snapshot(ViewModel),
    Patch(Vec<Delta>),
    Connection(ConnState),
    Error(UiError),
    /// Alert counts for the area selector lamps (sent when they change).
    AreaBadges(AreaBadges),
    /// Hidden-tab keep-alive (replaces patches while hidden).
    Heartbeat {
        seq: u32,
    },
}
