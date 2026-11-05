// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Messages exchanged by the UI module and the data worker.
//!
//! Identifiers and routes live here; the skin-agnostic view models carried
//! by [`DataEvent::Snapshot`] / [`DataEvent::Patch`] live in [`crate::view`].

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::config::WorkerConfig;
use crate::dto::AuthSessionDto;
use crate::view::{AreaBadges, DataSource, Delta, ViewModel};

/// Top-level console area (5 rail positions: `CI`, `CD`, `REG`, `OBS`, `FLT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    #[default]
    Ci,
    Cd,
    Registry,
    Observe,
    Fleet,
}

impl Area {
    /// All 5 console areas in canonical `C1` rail order.
    pub const ALL: [Self; 5] = [
        Self::Ci,
        Self::Cd,
        Self::Registry,
        Self::Observe,
        Self::Fleet,
    ];
}

/// Sub-views owned by [`Area::Ci`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiSubView {
    #[default]
    Scorecards,
    Trainway,
    QueueLogs,
}

impl CiSubView {
    pub const ALL: [Self; 3] = [Self::Scorecards, Self::Trainway, Self::QueueLogs];
}

/// Sub-views owned by [`Area::Cd`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdSubView {
    #[default]
    IntegrationGate,
    DeploymentsSlots,
    ActiveLeases,
}

impl CdSubView {
    pub const ALL: [Self; 3] = [
        Self::IntegrationGate,
        Self::DeploymentsSlots,
        Self::ActiveLeases,
    ];
}

/// Sub-views owned by [`Area::Registry`] (`REG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrySubView {
    #[default]
    Packages,
    #[serde(alias = "canaries_taints")]
    CanaryTaints,
    CasHealth,
}

impl RegistrySubView {
    /// Alias matching `CanariesTaints` naming.
    #[allow(non_upper_case_globals)]
    pub const CanariesTaints: Self = Self::CanaryTaints;

    pub const ALL: [Self; 3] = [Self::Packages, Self::CanaryTaints, Self::CasHealth];
}

/// Sub-views owned by [`Area::Observe`] (`OBS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserveSubView {
    #[default]
    Smartscape,
    PurePath,
    ProblemsSlos,
    Logs,
    Metrics,
    DataStore,
}

impl ObserveSubView {
    pub const ALL: [Self; 6] = [
        Self::Smartscape,
        Self::PurePath,
        Self::ProblemsSlos,
        Self::Logs,
        Self::Metrics,
        Self::DataStore,
    ];
}

/// Sub-views owned by [`Area::Fleet`] (`FLT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FleetSubView {
    #[default]
    WorkerNodes,
    SandboxProfiles,
}

impl FleetSubView {
    pub const ALL: [Self; 2] = [Self::WorkerNodes, Self::SandboxProfiles];
}

/// Active secondary sub-view across any of the 5 top-level areas (17 total).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubView {
    Ci(CiSubView),
    Cd(CdSubView),
    Registry(RegistrySubView),
    Observe(ObserveSubView),
    Fleet(FleetSubView),
}

impl Default for SubView {
    fn default() -> Self {
        Self::Ci(CiSubView::Scorecards)
    }
}

impl SubView {
    /// Default sub-view when switching into `area`.
    #[must_use]
    pub const fn default_for(area: Area) -> Self {
        match area {
            Area::Ci => Self::Ci(CiSubView::Scorecards),
            Area::Cd => Self::Cd(CdSubView::IntegrationGate),
            Area::Registry => Self::Registry(RegistrySubView::Packages),
            Area::Observe => Self::Observe(ObserveSubView::Smartscape),
            Area::Fleet => Self::Fleet(FleetSubView::WorkerNodes),
        }
    }

    /// Parent [`Area`] of this sub-view.
    #[must_use]
    pub const fn area(self) -> Area {
        match self {
            Self::Ci(_) => Area::Ci,
            Self::Cd(_) => Area::Cd,
            Self::Registry(_) => Area::Registry,
            Self::Observe(_) => Area::Observe,
            Self::Fleet(_) => Area::Fleet,
        }
    }

    /// Canonical URL slug for this sub-view (`scorecards`, `trainway`, `purepath`, etc.).
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Ci(CiSubView::Scorecards) => "scorecards",
            Self::Ci(CiSubView::Trainway) => "trainway",
            Self::Ci(CiSubView::QueueLogs) => "logs",
            Self::Cd(CdSubView::IntegrationGate) => "gate",
            Self::Cd(CdSubView::DeploymentsSlots) => "deployments",
            Self::Cd(CdSubView::ActiveLeases) => "leases",
            Self::Registry(RegistrySubView::Packages) => "packages",
            Self::Registry(RegistrySubView::CanaryTaints) => "canaries",
            Self::Registry(RegistrySubView::CasHealth) => "cas",
            Self::Observe(ObserveSubView::Smartscape) => "smartscape",
            Self::Observe(ObserveSubView::PurePath) => "purepath",
            Self::Observe(ObserveSubView::ProblemsSlos) => "problems",
            Self::Observe(ObserveSubView::Logs) => "logs",
            Self::Observe(ObserveSubView::Metrics) => "metrics",
            Self::Observe(ObserveSubView::DataStore) => "store",
            Self::Fleet(FleetSubView::WorkerNodes) => "nodes",
            Self::Fleet(FleetSubView::SandboxProfiles) => "sandboxes",
        }
    }

    /// Display label on the `SubViewBar`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ci(CiSubView::Scorecards) => "Scorecards",
            Self::Ci(CiSubView::Trainway) => "Trainway",
            Self::Ci(CiSubView::QueueLogs) => "Queue & Logs",
            Self::Cd(CdSubView::IntegrationGate) => "Integration Gate",
            Self::Cd(CdSubView::DeploymentsSlots) => "Deployments & Slots",
            Self::Cd(CdSubView::ActiveLeases) => "Active Leases",
            Self::Registry(RegistrySubView::Packages) => "Packages",
            Self::Registry(RegistrySubView::CanaryTaints) => "Canary Bundles & Taints",
            Self::Registry(RegistrySubView::CasHealth) => "CAS Storage Health",
            Self::Observe(ObserveSubView::Smartscape) => "Smartscape",
            Self::Observe(ObserveSubView::PurePath) => "PurePath",
            Self::Observe(ObserveSubView::ProblemsSlos) => "Problems & SLOs",
            Self::Observe(ObserveSubView::Logs) => "Logs",
            Self::Observe(ObserveSubView::Metrics) => "Metrics",
            Self::Observe(ObserveSubView::DataStore) => "Data Store",
            Self::Fleet(FleetSubView::WorkerNodes) => "Worker Nodes",
            Self::Fleet(FleetSubView::SandboxProfiles) => "Sandbox Profiles",
        }
    }

    /// Ordered sub-views for `area`.
    #[must_use]
    pub const fn all_for(area: Area) -> &'static [Self] {
        match area {
            Area::Ci => &[
                Self::Ci(CiSubView::Scorecards),
                Self::Ci(CiSubView::Trainway),
                Self::Ci(CiSubView::QueueLogs),
            ],
            Area::Cd => &[
                Self::Cd(CdSubView::IntegrationGate),
                Self::Cd(CdSubView::DeploymentsSlots),
                Self::Cd(CdSubView::ActiveLeases),
            ],
            Area::Registry => &[
                Self::Registry(RegistrySubView::Packages),
                Self::Registry(RegistrySubView::CanaryTaints),
                Self::Registry(RegistrySubView::CasHealth),
            ],
            Area::Observe => &[
                Self::Observe(ObserveSubView::Smartscape),
                Self::Observe(ObserveSubView::PurePath),
                Self::Observe(ObserveSubView::ProblemsSlos),
                Self::Observe(ObserveSubView::Logs),
                Self::Observe(ObserveSubView::Metrics),
                Self::Observe(ObserveSubView::DataStore),
            ],
            Area::Fleet => &[
                Self::Fleet(FleetSubView::WorkerNodes),
                Self::Fleet(FleetSubView::SandboxProfiles),
            ],
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
            area: Area::Ci,
            subview: SubView::Ci(CiSubView::Scorecards),
            repo: None,
            submodule: None,
            branch: None,
            job: None,
            trace_id: None,
            filter: None,
        }
    }
}

/// CRT terminal font family. The layout solver re-measures on change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrtFont {
    /// Hurmit Nerd Font Mono (default).
    Hurmit,
    /// OpenDyslexicM Nerd Font Mono (accessibility switch).
    OpenDyslexic,
}

/// Panels that support panel-local paging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelId {
    Carousel,
    Trainway,
    Delivery,
    Registry,
    Observe,
    Fleet,
    RepoPicker,
    Log,
}

/// UI -> data worker.
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
    /// Redeem a one-time operator invite token (`POST /api/auth/redeem`).
    RedeemInvite {
        token: String,
    },
    /// Directly set the operator authentication session state.
    SetAuthSession(AuthSessionDto),
    /// Restart / retry a specific CI job (`POST /jobs/{id}/restart`).
    RestartJob {
        job_id: u64,
    },
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
