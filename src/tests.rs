// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Property tests: codec round trips and `apply(old, diff(old, new)) == new`.
//!
//! A small deterministic xorshift generator stands in for a property-testing
//! framework so the crate keeps zero dev-dependencies.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next() % u64::from(n.max(1))).unwrap_or(0)
    }
    fn len(&mut self, max: u32) -> usize {
        usize::try_from(self.below(max + 1)).unwrap_or(0)
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        let i = usize::try_from(self.next() % items.len() as u64).unwrap_or(0);
        items[i]
    }
    fn name(&mut self, prefix: &str, space: u32) -> String {
        format!("{prefix}{}", self.below(space))
    }
}

const STAGES: [StageState; 5] = [
    StageState::Pending,
    StageState::Running,
    StageState::Passed,
    StageState::Failed,
    StageState::Skipped,
];
const TESTS: [TestState; 4] = [
    TestState::Pending,
    TestState::Running,
    TestState::Passed,
    TestState::Failed,
];
const OUTCOMES: [GateOutcome; 4] = [
    GateOutcome::Pending,
    GateOutcome::Promoted,
    GateOutcome::RolledBack,
    GateOutcome::Tainted,
];

fn sort_dedup<T, K: Ord + Clone>(mut v: Vec<T>, key: impl Fn(&T) -> K) -> Vec<T> {
    v.sort_by_key(|x| key(x));
    v.dedup_by_key(|x| key(x));
    v
}

fn gen_repo(r: &mut Rng) -> RepoScore {
    let mut s = RepoScore::new(RepoId::new(&r.name("o", 2), &r.name("r", 6)));
    s.status = r.pick(&[
        RepoStatus::Unknown,
        RepoStatus::Passing,
        RepoStatus::Running,
        RepoStatus::Failing,
    ]);
    s.score = r.below(101);
    s.coverage_bp = r.below(10_001);
    s.bench_delta_bp = i32::try_from(r.below(400)).unwrap_or(0) - 200;
    s.latest_job_id = r.coin().then(|| r.next() % 10_000);
    for _ in 0..r.len(2) {
        s.submodules.push(SubmoduleRef {
            path: r.name("code/m", 9),
            repo: RepoId::new("o0", &r.name("sub", 9)),
            pinned_sha: r.name("sha", 99),
        });
    }
    if r.coin() {
        s.cd = Some(CdScore {
            deployment_id: r.name("d", 9),
            active_slot: r.pick(&[Slot::A, Slot::B]),
            outcome: r.pick(&OUTCOMES),
            it_passed: r.below(5),
            it_total: 5,
        });
    }
    s
}

fn gen_topology(r: &mut Rng) -> RepoTopology {
    let stations: Vec<Station> = (0..r.pick(&[3_u32, 8]))
        .map(|i| Station {
            id: format!("s{i}"),
            label: format!("S{i}"),
        })
        .collect();
    let n_st = u32::try_from(stations.len()).unwrap_or(1);
    let branches = (0..r.len(4))
        .map(|_| Branch {
            ref_name: r.name("feat/b", 8),
            base_ref: "main".into(),
            fork_stage: r.below(n_st),
            pr_state: r.pick(&[PrState::None, PrState::Open, PrState::Merged]),
            last_job: r.coin().then(|| r.next() % 100),
            phase: match r.below(3) {
                0 => SidingPhase::UnderConstruction,
                1 => SidingPhase::Active,
                _ => SidingPhase::Fading {
                    reason: FadeReason::Merged,
                    since_epoch_ms: r.next() % 1_000,
                },
            },
        })
        .collect();
    let trains = (0..r.len(4))
        .map(|_| Train {
            job_id: u64::from(r.below(12)),
            ref_name: r.name("feat/b", 8),
            stage: r.below(n_st),
            stage_states: stations.iter().map(|_| r.pick(&STAGES)).collect(),
            node_id: r.coin().then(|| r.name("node-", 3)),
            node_lost: r.coin(),
        })
        .collect();
    RepoTopology {
        repo: RepoId::new("o0", &r.name("r", 2)),
        submodule: r.coin().then(|| SubmoduleId {
            path: r.name("code/m", 2),
        }),
        default_branch: "main".into(),
        stations,
        branches: sort_dedup(branches, |b| b.ref_name.clone()),
        trains: sort_dedup(trains, |t| t.job_id),
        submodules: Vec::new(),
        fabric_rev: r.next() % 500,
        submodule_bump_pending: r.coin(),
    }
}

fn gen_gate(r: &mut Rng) -> DeploymentGate {
    let tests = (0..r.len(6))
        .map(|i| GateTest {
            name: format!("it_{i}"),
            critical: r.coin(),
            state: r.pick(&TESTS),
            duration_ms: r.next() % 5_000,
            crate_name: r.coin().then(|| r.name("k3-", 3)),
            log_excerpt: r.coin().then(|| r.name("log ", 9)),
        })
        .collect();
    DeploymentGate {
        deployment_id: r.name("d", 2),
        repo: RepoId::new("o0", "r0"),
        service: "svc".into(),
        artifact_digest: r.name("sha256:", 2),
        target_node: r.name("node-", 2),
        staged_slot: r.pick(&[Slot::A, Slot::B]),
        restore_slot: r.pick(&[Slot::A, Slot::B]),
        lease_expires_epoch_ms: r.next() % 10_000,
        outcome: r.pick(&OUTCOMES),
        rollback_reason: r.coin().then(|| {
            r.pick(&[
                RollbackReason::CriticalTestFailed,
                RollbackReason::LeaseExpired,
                RollbackReason::Manual,
            ])
        }),
        tainted: r.coin(),
        tests,
    }
}

fn gen_view(r: &mut Rng) -> ViewModel {
    let repos = (0..r.len(6)).map(|_| gen_repo(r)).collect();
    let nodes = (0..r.len(3))
        .map(|_| FabricNode::new(r.name("node-", 4), r.coin()))
        .collect();
    ViewModel {
        source: match r.below(3) {
            0 => DataSource::Live,
            1 => DataSource::Scenario("x".into()),
            _ => DataSource::OfflineCache,
        },
        repos: sort_dedup(repos, |x: &RepoScore| x.id.clone()),
        selection: r.coin().then(|| Selection {
            repo: RepoId::new("o0", &r.name("r", 2)),
            submodule: None,
        }),
        topology: r.coin().then(|| gen_topology(r)),
        gate: r.coin().then(|| gen_gate(r)),
        history: (0..r.len(3)).map(|_| gen_gate(r).summary()).collect(),
        nodes: sort_dedup(nodes, |n: &FabricNode| n.id.clone()),
        stale_since_epoch_ms: r.coin().then(|| r.next() % 100),
        leases: Vec::new(),
        sandboxes: Vec::new(),
        registry: RegistryView::default(),
        observe: ObserveView::default(),
        auth: match r.below(3) {
            0 => AuthState::Anonymous,
            1 => AuthState::InviteRequired {
                github_login: r.name("guest-", 4),
            },
            _ => AuthState::Operator {
                github_login: r.name("op-", 4),
                role: r.coin().then(|| "operator".into()),
            },
        },
        ci_jobs: Vec::new(),
    }
}

/// Small edit of `vm` so that most diffs exercise the incremental paths.
fn mutate(r: &mut Rng, vm: &ViewModel) -> ViewModel {
    let mut out = vm.clone();
    for _ in 0..=r.below(4) {
        match r.below(8) {
            0 => {
                let repo = gen_repo(r);
                match out.repos.binary_search_by(|x| x.id.cmp(&repo.id)) {
                    Ok(i) => out.repos[i] = repo,
                    Err(i) => out.repos.insert(i, repo),
                }
            }
            1 if !out.repos.is_empty() => {
                let i = r.len(u32::try_from(out.repos.len() - 1).unwrap_or(0));
                out.repos.remove(i);
            }
            2 => {
                if let Some(t) = out.topology.as_mut() {
                    let extra = gen_topology(r);
                    let mut b = t.branches.clone();
                    b.extend(extra.branches);
                    t.branches = sort_dedup(b, |x| x.ref_name.clone());
                    t.fabric_rev += 1;
                }
            }
            3 => {
                if let Some(t) = out.topology.as_mut() {
                    if let Some(tr) = t.trains.first_mut() {
                        tr.node_lost = !tr.node_lost;
                        tr.stage = 0;
                    } else {
                        t.trains.clear();
                    }
                    t.submodule_bump_pending = !t.submodule_bump_pending;
                }
            }
            4 => {
                if let Some(g) = out.gate.as_mut() {
                    if let Some(t) = g.tests.last_mut() {
                        t.state = r.pick(&TESTS);
                        t.duration_ms += 1;
                    }
                    g.tests.push(GateTest {
                        name: format!("it_new_{}", g.tests.len()),
                        critical: true,
                        state: TestState::Running,
                        duration_ms: 0,
                        crate_name: None,
                        log_excerpt: None,
                    });
                    g.outcome = r.pick(&OUTCOMES);
                }
            }
            5 => out.nodes.retain(|n| n.online),
            6 => out.stale_since_epoch_ms = None,
            _ => out = gen_view(r),
        }
    }
    out
}

#[test]
fn diff_then_apply_reconstructs_the_target() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let cases = if cfg!(miri) { 16 } else { 2_000 };
    for case in 0..cases {
        let old = gen_view(&mut r);
        let new = if case % 3 == 0 {
            gen_view(&mut r)
        } else {
            mutate(&mut r, &old)
        };
        let patch = diff(&old, &new);
        let mut mirror = old.clone();
        apply_all(&mut mirror, &patch);
        assert_eq!(mirror, new, "case {case}: patch {patch:?}");
        assert!(diff(&new, &new).is_empty(), "case {case}: self diff");
    }
}

#[test]
fn incremental_diffs_stay_incremental() {
    let mut r = Rng(7);
    let mut old = gen_view(&mut r);
    old.topology = Some(gen_topology(&mut r));
    let mut new = old.clone();
    if let Some(t) = new.topology.as_mut() {
        t.fabric_rev += 1;
    }
    assert_eq!(
        diff(&old, &new),
        alloc::vec![Delta::FabricRev(
            new.topology.as_ref().map_or(0, |t| t.fabric_rev)
        )]
    );
}

#[test]
fn random_events_round_trip_through_the_codec() {
    let mut r = Rng(42);
    let iterations = if cfg!(miri) { 8 } else { 500 };
    for _ in 0..iterations {
        let a = gen_view(&mut r);
        let b = mutate(&mut r, &a);
        for ev in [
            DataEvent::Snapshot(a.clone()),
            DataEvent::Patch(diff(&a, &b)),
        ] {
            let frame = encode(&ev).expect("encode");
            assert_eq!(decode::<DataEvent>(&frame), Ok(ev.clone()));
            // Every strict prefix fails closed instead of panicking.
            let cut = r.len(u32::try_from(frame.len() - 1).unwrap_or(0));
            assert!(decode::<DataEvent>(&frame[..cut]).is_err());
        }
    }
}

#[test]
fn gate_lamps_follow_critical_tests() {
    let mut r = Rng(3);
    let mut g = gen_gate(&mut r);
    g.outcome = GateOutcome::Pending;
    g.tests = (0..3)
        .map(|i| GateTest {
            name: format!("it_{i}"),
            critical: i != 2,
            state: TestState::Passed,
            duration_ms: 10,
            crate_name: None,
            log_excerpt: None,
        })
        .collect();
    g.tests[2].state = TestState::Failed;
    assert!(g.promote_lit(), "non-critical failure does not block");
    assert!(!g.rollback_lit());
    assert_eq!((g.passed(), g.total()), (2, 3));
    g.tests[0].state = TestState::Failed;
    assert!(!g.promote_lit());
    assert!(g.rollback_lit());
    assert_eq!(g.summary().it_passed, 1);
    assert!(GateOutcome::Tainted.is_final() && !GateOutcome::Pending.is_final());
    assert_eq!(Slot::A.other(), Slot::B);
}

#[test]
fn stop_and_queue_flags() {
    let mut r = Rng(11);
    let mut t = gen_topology(&mut r);
    t.trains = alloc::vec![Train {
        job_id: 1,
        ref_name: "feat/x".into(),
        stage: 0,
        stage_states: alloc::vec![StageState::Passed, StageState::Running],
        node_id: None,
        node_lost: false,
    }];
    assert!(!t.stop());
    assert!(t.trains[0].queued());
    t.trains[0].stage_states[1] = StageState::Failed;
    assert!(t.stop());
}

#[test]
fn deltas_on_absent_targets_are_ignored() {
    let mut vm = ViewModel::default();
    apply_all(
        &mut vm,
        &[
            Delta::FabricRev(9),
            Delta::TrainRemoved(1),
            Delta::GateStatus {
                outcome: GateOutcome::Promoted,
                rollback_reason: None,
                tainted: false,
                lease_expires_epoch_ms: 0,
            },
            Delta::NodeRemoved("node-a".into()),
        ],
    );
    assert_eq!(vm, ViewModel::default());
}

#[test]
fn sanitize_public_log_redacts_private_ips_hosts_and_paths() {
    let raw = "connect 192.0.2.42 on test-node-alpha.internal.local and node-b /var/lib/action-runner/.cargo/bin/cargo";
    let clean = sanitize_public_log(raw);
    assert!(clean.contains("192.0.###.42"), "clean: {clean}");
    assert!(!clean.contains("192.0.2.42"), "clean: {clean}");
    assert!(!clean.contains(".internal.local"), "clean: {clean}");
    assert!(clean.contains(".../bin/cargo"), "clean: {clean}");
}

#[test]
fn subviews_round_trip_and_step_within_area() {
    for area in Area::ALL {
        let views = SubView::all_for(area);
        assert!(views.len() >= 2);
        assert_eq!(SubView::default_for(area), views[0]);
        for (idx, sv) in views.iter().copied().enumerate() {
            assert_eq!(sv.area(), area);
            assert!(!sv.label().is_empty());
            let next = sv.step(1);
            assert_eq!(next, views[(idx + 1) % views.len()]);
            let prev = next.step(-1);
            assert_eq!(prev, sv);
        }
    }
}

#[test]
fn auth_state_and_commands_round_trip_through_postcard() {
    use crate::dto::AuthSessionDto;

    let anon = AuthState::from_session(&AuthSessionDto::default());
    assert_eq!(anon, AuthState::Anonymous);
    assert!(!anon.is_operator());
    assert_eq!(anon.badge_label(), "READ-ONLY");

    let uninvited = AuthState::from_session(&AuthSessionDto {
        authenticated: false,
        github_login: Some("guest-dev".into()),
        invite_bound: false,
        role: None,
    });
    assert_eq!(
        uninvited,
        AuthState::InviteRequired {
            github_login: "guest-dev".into()
        }
    );
    assert!(!uninvited.is_operator());
    assert_eq!(uninvited.badge_label(), "INVITE REQ (@guest-dev)");

    let op = AuthState::from_session(&AuthSessionDto {
        authenticated: true,
        github_login: Some("LMA1980".into()),
        invite_bound: true,
        role: Some("admin".into()),
    });
    assert_eq!(
        op,
        AuthState::Operator {
            github_login: "LMA1980".into(),
            role: Some("admin".into()),
        }
    );
    assert!(op.is_operator());
    assert_eq!(op.badge_label(), "@LMA1980");

    for cmd in [
        UiCommand::RedeemInvite {
            token: "inv_019a_secret".into(),
        },
        UiCommand::SetAuthSession(AuthSessionDto {
            authenticated: true,
            github_login: Some("LMA1980".into()),
            invite_bound: true,
            role: Some("operator".into()),
        }),
        UiCommand::RestartJob { job_id: 42 },
        UiCommand::ResumeJob { job_id: 42 },
        UiCommand::SelectJob { job_id: 42 },
    ] {
        let bytes = encode(&cmd).expect("encode cmd");
        assert_eq!(decode::<UiCommand>(&bytes), Ok(cmd));
    }
}

#[test]
fn queue_item_view_unwrapping_ansi_log_sanitization_and_paths() {
    use crate::dto::{
        paths, CiQueueJobInnerDto, CiQueueJobRecordDto, CiQueueJobSpecDto, CiQueueStageDetailsDto,
        CiQueueStageDto, CiQueueStageStatusDto,
    };

    let wrapped = CiQueueJobRecordDto {
        elapsed_secs: 26,
        wait_secs: 3,
        position: 0,
        record: Some(CiQueueJobInnerDto {
            job: CiQueueJobSpecDto {
                id: "job-1794054805-5cec9056".into(),
                repo_name: "LMA1980/mu-console".into(),
                ref_name: "refs/heads/main".into(),
                commit_sha: "5cec90567890".into(),
                event_type: "push".into(),
                queued_at: "2026-11-07T12:00:00Z".into(),
            },
            status: CiQueueStageStatusDto {
                state: "Running".into(),
                details: CiQueueStageDetailsDto::default(),
            },
            metrics: None,
            stages: alloc::vec![CiQueueStageDto {
                id: "fmt-check".into(),
                name: "Verify Rust Formatting".into(),
                assigned_node: Some("worker-riscv64".into()),
                threads: 6,
                status: CiQueueStageStatusDto {
                    state: "Completed".into(),
                    details: CiQueueStageDetailsDto {
                        duration_secs: 4,
                        exit_code: 0,
                    },
                },
            }],
        }),
        ..CiQueueJobRecordDto::default()
    };
    let rec = wrapped.into_unwrapped();
    assert_eq!(rec.job.id, "job-1794054805-5cec9056");
    assert_eq!(rec.status.state, "Running");
    assert_eq!(rec.elapsed_secs, 26);
    assert_eq!(rec.wait_secs, 3);
    assert_eq!(rec.stages.len(), 1);
    assert_eq!(
        StageState::from_status_str(&rec.stages[0].status.state, 0),
        StageState::Passed
    );
    assert_eq!(StageState::Passed.label(), "PASSED");

    let ansi = "\x1b[32m   Compiling\x1b[0m mu-console v0.1.0 (/var/lib/action-runner/work/mu-console)\n\x1b[1;31merror\x1b[0m: failed on 192.0.2.15\n";
    let lines = sanitize_ansi_log_lines(ansi, 10);
    assert_eq!(lines.len(), 2);
    assert!(!lines[0].contains("\x1b"));
    assert!(lines[0].contains(".../work/mu-console"));
    assert!(lines[1].contains("192.0.###.15"));

    assert_eq!(
        paths::job_restart_id("job-1794054805-5cec9056"),
        "/api/jobs/job-1794054805-5cec9056/restart"
    );
    assert_eq!(
        paths::job_resume_id("job-1794054805-5cec9056"),
        "/api/jobs/job-1794054805-5cec9056/resume"
    );
    assert_eq!(
        paths::job_detail_id("job-1794054805-5cec9056"),
        "/api/jobs/job-1794054805-5cec9056"
    );
    assert_eq!(
        paths::job_log_id("job-1794054805-5cec9056"),
        "/api/jobs/job-1794054805-5cec9056/log"
    );
    assert_eq!(
        paths::job_obs_log_id("job-1794054805-5cec9056"),
        "/api/obs/v1/jobs/job-1794054805-5cec9056/log"
    );
    assert_eq!(DEFAULT_MAX_JOB_LOG_LINES, 2_000);
}

#[test]
fn raw_metric_points_aggregate_into_series_alongside_preaggregated_rows() {
    use crate::dto::{MetricSeriesDto, RawMetricPointDto};

    let raw_points = alloc::vec![
        RawMetricPointDto {
            timestamp_ns: 1_700_000_002_000_000_000,
            service: "muobserve-collector".into(),
            node_id: "worker-riscv64".into(),
            metric_name: "host.psi.memory.some_avg60".into(),
            metric_unit: "%".into(),
            value_f64: 0.42,
            arch: "riscv64".into(),
            cpu_cluster: "X100".into(),
            vlen_bits: 256,
            ..RawMetricPointDto::default()
        },
        RawMetricPointDto {
            timestamp_ns: 1_700_000_001_000_000_000,
            service: "muobserve-collector".into(),
            node_id: "worker-riscv64".into(),
            metric_name: "host.psi.memory.some_avg60".into(),
            metric_unit: "%".into(),
            value_f64: 0.18,
            arch: "riscv64".into(),
            cpu_cluster: "X100".into(),
            vlen_bits: 256,
            ..RawMetricPointDto::default()
        },
        RawMetricPointDto {
            timestamp_ns: 1_700_000_003_000_000_000,
            service: "muobserve-collector".into(),
            node_id: "node-a".into(),
            metric_name: "host.load1".into(),
            metric_unit: String::new(),
            value_f64: 1.75,
            arch: "x86_64".into(),
            cpu_cluster: "unknown".into(),
            vlen_bits: 0,
            ..RawMetricPointDto::default()
        },
        RawMetricPointDto {
            service: "k3-kernels-rvv".into(),
            hardware_class: "Cluster 1 A100 (VLEN=1024)".into(),
            rps: 118,
            error_rate_bp: 0,
            p50_ms: 11,
            p95_ms: 14,
            p99_ms: 17,
            exemplar_trace_id: "tr-obs-p99-07".into(),
            samples: alloc::vec![11, 14, 17],
            ..RawMetricPointDto::default()
        },
    ];

    let series = RawMetricPointDto::aggregate_into_series(&raw_points);
    assert_eq!(series.len(), 3);

    // Pre-aggregated series is preserved first.
    assert_eq!(
        series[0],
        MetricSeriesDto {
            service: "k3-kernels-rvv".into(),
            hardware_class: "Cluster 1 A100 (VLEN=1024)".into(),
            rps: 118,
            error_rate_bp: 0,
            p50_ms: 11,
            p95_ms: 14,
            p99_ms: 17,
            exemplar_trace_id: "tr-obs-p99-07".into(),
            samples: alloc::vec![11, 14, 17],
        }
    );

    // Raw points for host.psi.memory.some_avg60 sorted by timestamp_ns and scaled by 100.
    assert_eq!(
        series[1].service,
        "muobserve-collector · host.psi.memory.some_avg60 (%)"
    );
    assert_eq!(
        series[1].hardware_class,
        "worker-riscv64 · riscv64 · X100 · rvv-256"
    );
    assert_eq!(series[1].rps, 2);
    assert_eq!(series[1].samples, alloc::vec![18, 42]);
    assert_eq!(series[1].p50_ms, 42);
    assert_eq!(series[1].p95_ms, 42);
    assert_eq!(series[1].p99_ms, 42);

    // Raw points for host.load1 with unknown cpu_cluster omitted.
    assert_eq!(series[2].service, "muobserve-collector · host.load1");
    assert_eq!(series[2].hardware_class, "node-a · x86_64");
    assert_eq!(series[2].rps, 1);
    assert_eq!(series[2].samples, alloc::vec![175]);
    assert_eq!(series[2].p50_ms, 175);
}
