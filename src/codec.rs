// SPDX-License-Identifier: LGPL-3.0-only WITH LGPL-3.0-linking-exception
//! Versioned postcard codec.
//!
//! Wire format: `varint(u32 PROTOCOL_VERSION) ++ postcard(body)`.

use alloc::vec::Vec;
use core::fmt;
use serde::{de::DeserializeOwned, Serialize};

/// Protocol version carried by every frame. Bump on any breaking change.
///
/// History: 1 = Phase 1 skeleton; 2 = Phase 2 data plane (full view models);
/// 3 = Phase 3 live node telemetry & CI queue; 4 = skin switcher & job restart POST;
/// 5 = Live workflow parity (QueueItemView unwrapping, per-job detail/log, ResumeJob, submodules);
/// 6 = Html & Wireframe skins, 8 role-gated areas, DialogModel, mu-auth folios/leases,
/// SubsystemHealthSummary, ecosystem-agnostic CdRunDto & Model-Runner DTOs.
pub const PROTOCOL_VERSION: u32 = 6;

/// Codec failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// The frame was produced by a different protocol version.
    VersionMismatch { expected: u32, found: u32 },
    /// Encoding failed.
    Encode,
    /// The frame is truncated or malformed.
    Decode,
    /// Bytes remained after the body was decoded.
    TrailingBytes { count: usize },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionMismatch { expected, found } => {
                write!(
                    f,
                    "protocol version mismatch: expected {expected}, found {found}"
                )
            }
            Self::Encode => f.write_str("postcard encode failed"),
            Self::Decode => f.write_str("postcard decode failed"),
            Self::TrailingBytes { count } => write!(f, "{count} trailing bytes after frame"),
        }
    }
}

/// Encode a message as a versioned frame.
///
/// # Errors
/// Returns [`CodecError::Encode`] if serialization fails.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, CodecError> {
    let mut out = postcard::to_allocvec(&PROTOCOL_VERSION).map_err(|_| CodecError::Encode)?;
    let body = postcard::to_allocvec(msg).map_err(|_| CodecError::Encode)?;
    out.extend_from_slice(&body);
    Ok(out)
}

/// Read only the protocol version of a frame.
///
/// # Errors
/// Returns [`CodecError::Decode`] if the frame has no valid version prefix.
pub fn peek_version(frame: &[u8]) -> Result<u32, CodecError> {
    postcard::take_from_bytes::<u32>(frame)
        .map(|(v, _)| v)
        .map_err(|_| CodecError::Decode)
}

/// Decode a versioned frame, rejecting other protocol versions.
///
/// # Errors
/// [`CodecError::VersionMismatch`] when the version differs,
/// [`CodecError::Decode`] for malformed input and
/// [`CodecError::TrailingBytes`] when the frame is longer than its body.
pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, CodecError> {
    let (found, rest) = postcard::take_from_bytes::<u32>(frame).map_err(|_| CodecError::Decode)?;
    if found != PROTOCOL_VERSION {
        return Err(CodecError::VersionMismatch {
            expected: PROTOCOL_VERSION,
            found,
        });
    }
    let (msg, tail) = postcard::take_from_bytes::<T>(rest).map_err(|_| CodecError::Decode)?;
    if !tail.is_empty() {
        return Err(CodecError::TrailingBytes { count: tail.len() });
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn repo(owner: &str, name: &str) -> RepoId {
        RepoId {
            owner: owner.to_string(),
            repo: name.to_string(),
        }
    }

    fn commands() -> Vec<UiCommand> {
        vec![
            UiCommand::Configure(crate::WorkerConfig {
                api_base: "/edge".to_string(),
                scenario: Some("tainted".to_string()),
            }),
            UiCommand::Subscribe(Route {
                area: Area::Ci,
                subview: SubView::Ci(CiSubView::Trainway),
                repo: Some(repo("LMA1980", "MuConsole")),
                submodule: None,
                branch: Some("feat/phase1-foundation".to_string()),
                job: Some(1234),
                trace_id: Some("tr-ci-1234".to_string()),
                filter: Some("ERRORS".to_string()),
            }),
            UiCommand::SelectSubView(SubView::Observability(ObserveSubView::PurePath)),
            UiCommand::ResetStream,
            UiCommand::SelectRepo {
                repo: repo("LMA1980", "model-runner"),
                submodule: Some(SubmoduleId {
                    path: "code/engine".to_string(),
                }),
            },
            UiCommand::SelectRepo {
                repo: repo("LMA1980", "MuRegist"),
                submodule: None,
            },
            UiCommand::Page {
                panel: PanelId::Trainway,
                cursor: 3,
            },
            UiCommand::Retry,
            UiCommand::SetVisibility { hidden: true },
            UiCommand::SetCrtFont(CrtFont::Hurmit),
            UiCommand::SetCrtFont(CrtFont::OpenDyslexic),
            UiCommand::SetSkin(ConsoleSkin::Html),
            UiCommand::SetSkin(ConsoleSkin::Wireframe),
            UiCommand::RestartJob { job_id: 101 },
            UiCommand::ResumeJob { job_id: 102 },
            UiCommand::SelectJob { job_id: 103 },
            UiCommand::EnableRole {
                role_id: "operator".to_string(),
            },
            UiCommand::DisableRole {
                role_id: "operator".to_string(),
            },
            UiCommand::ElevateWithPasskey {
                role_id: "admin".to_string(),
                assertion_json: "{}".to_string(),
            },
            UiCommand::LockTimedRoles,
            UiCommand::RecordUserActivity,
            UiCommand::UpdateUserProfile {
                display_name: Some("Operator".to_string()),
                preferred_skin: Some(ConsoleSkin::Html),
                preferred_font: Some(CrtFont::Hurmit),
            },
            UiCommand::OpenDialog(DialogModel::PasskeyEnrollmentGate(
                PasskeyEnrollmentGateModel {
                    github_login: "lma".to_string(),
                    passkey_count: 0,
                    reason_code: Some(AuthReasonCode::PasskeyEnrollmentRequired),
                },
            )),
            UiCommand::CloseDialog,
            UiCommand::TriggerAbRollback {
                deployment_id: "dep-42".to_string(),
                note: Some("canary error spike".to_string()),
            },
            UiCommand::SetSubsystemMaintenance {
                subsystem: SubsystemId::Cd,
                mode: MaintenanceMode::Manual(SubsystemStatus::InMaintenance),
                reason: Some("schema rollout".to_string()),
                auto_clear_secs: Some(300),
            },
        ]
    }

    fn events() -> Vec<DataEvent> {
        let mut score = RepoScore::new(repo("LMA1980", "MuConsole"));
        score.status = RepoStatus::Running;
        score.score = 81;
        score.coverage_bp = 8_300;
        vec![
            DataEvent::Ready {
                protocol: PROTOCOL_VERSION,
                source: DataSource::Scenario("node-lost".to_string()),
            },
            DataEvent::Snapshot(ViewModel {
                repos: vec![score.clone()],
                ..ViewModel::default()
            }),
            DataEvent::Snapshot(ViewModel::default()),
            DataEvent::Patch(vec![
                Delta::FabricRev(483),
                Delta::RepoUpsert(score),
                Delta::RepoRemoved(repo("LMA1980", "old")),
            ]),
            DataEvent::Connection(ConnState::Reconnecting {
                attempt: 2,
                retry_in_ms: 1_000,
            }),
            DataEvent::Connection(ConnState::Live),
            DataEvent::Connection(ConnState::Paused),
            DataEvent::Error(UiError::VersionMismatch {
                expected: 1,
                found: 2,
            }),
            DataEvent::Error(UiError::Http {
                status: 503,
                message: "upstream".to_string(),
            }),
            DataEvent::Error(UiError::UnknownScenario("nope".to_string())),
            DataEvent::Error(UiError::AuthDenied(AuthReasonCode::AuthorizationDenied)),
            DataEvent::AreaBadges(AreaBadges {
                dashboard: 0,
                ci: 2,
                cd: 1,
                ab_deploy: 1,
                repository: 1,
                observability: 2,
                model_runner: 0,
                platform: 1,
            }),
            DataEvent::Heartbeat { seq: 7 },
        ]
    }

    #[test]
    fn ui_commands_round_trip() {
        for cmd in commands() {
            let frame = encode(&cmd).expect("encode");
            assert_eq!(peek_version(&frame), Ok(PROTOCOL_VERSION));
            let back: UiCommand = decode(&frame).expect("decode");
            assert_eq!(back, cmd);
        }
    }

    #[test]
    fn data_events_round_trip() {
        for ev in events() {
            let frame = encode(&ev).expect("encode");
            let back: DataEvent = decode(&frame).expect("decode");
            assert_eq!(back, ev);
        }
    }

    #[test]
    fn version_mismatch_is_reported() {
        let mut frame = postcard::to_allocvec(&(PROTOCOL_VERSION + 1)).expect("version");
        frame.extend(postcard::to_allocvec(&UiCommand::Retry).expect("body"));
        assert_eq!(peek_version(&frame), Ok(PROTOCOL_VERSION + 1));
        assert_eq!(
            decode::<UiCommand>(&frame),
            Err(CodecError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                found: PROTOCOL_VERSION + 1
            })
        );
    }

    #[test]
    fn malformed_frames_fail_closed() {
        assert_eq!(decode::<UiCommand>(&[]), Err(CodecError::Decode));
        let frame = encode(&UiCommand::SetVisibility { hidden: false }).expect("encode");
        assert_eq!(
            decode::<UiCommand>(&frame[..frame.len() - 1]),
            Err(CodecError::Decode)
        );
        let mut long = frame.clone();
        long.push(0);
        assert_eq!(
            decode::<UiCommand>(&long),
            Err(CodecError::TrailingBytes { count: 1 })
        );
    }

    #[test]
    fn select_repo_frame_is_compact() {
        let frame = encode(&UiCommand::SelectRepo {
            repo: repo("LMA1980", "MuRegist"),
            submodule: None,
        })
        .expect("encode");
        // version + variant + two short strings + Option tag
        assert!(frame.len() < 24, "frame is {} bytes", frame.len());
    }

    #[test]
    fn error_display_is_readable() {
        let e = CodecError::VersionMismatch {
            expected: 1,
            found: 9,
        };
        assert_eq!(
            e.to_string(),
            "protocol version mismatch: expected 1, found 9"
        );
    }

    #[test]
    fn rejects_removed_skin_discriminant() {
        let html_frame = encode(&UiCommand::SetSkin(ConsoleSkin::Html)).expect("encode html");
        assert_eq!(*html_frame.last().expect("skin discriminant byte"), 0);
        let mut frame =
            encode(&UiCommand::SetSkin(ConsoleSkin::Wireframe)).expect("encode wireframe");
        let last = frame.last_mut().expect("skin discriminant byte");
        assert_eq!(*last, 1);
        *last = 2;
        assert_eq!(decode::<UiCommand>(&frame), Err(CodecError::Decode));
    }
}
