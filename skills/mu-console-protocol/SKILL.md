______________________________________________________________________

## name: mu-console-protocol description: "Implement and consume the mu-console-protocol crate for MuConsole UI \<-> data worker IPC, versioned postcard codec, and Axum DTO mappings."

# `mu-console-protocol` Crate Architecture & Guidelines

Use these guidelines when modifying or consuming `mu-console-protocol` for IPC communication between the MuConsole UI (main browser thread) and dedicated Web Worker threads:

## 1. Hybrid Wire & Frame Architecture

- **Postcard Framing (`codec`)**:
  - The UI module (main thread) and the data module (dedicated Web Worker) exchange `UiCommand` and `DataEvent` messages as postcard bytes in transferable `ArrayBuffer`s.
  - Every frame starts with a `u32` protocol version (`PROTOCOL_VERSION`).
  - Wire format: `varint(u32 PROTOCOL_VERSION) ++ postcard(body)`.
  - Stale worker or UI bundles are detected via `CodecError::VersionMismatch` to trigger a graceful reload banner rather than panic.

## 2. Protocol Components

- **Commands, Areas, Skins & Dialogs (`model`)**:
  - `PROTOCOL_VERSION = 6`; `ConsoleSkin::{Html, Wireframe}` (`Html` default).
  - 8 role-gated `Area` variants (`Dashboard`, `Ci`, `Cd`, `AbDeploy`, `Repository`, `Observability`, `ModelRunner`, `Platform`) across 27 `SubView` variants, mapping legacy `#/iam`, `#/fleet`, `#/cd/gate`, `#/registry`, and `#/observe` routes cleanly.
  - Skin-agnostic `DialogModel` (`PasskeyEnrollmentGate`, `UserProfile`, `SessionRoles`, `RiskConfirm`, `InviteCreatedOnce`), `RoleLeaseView`, `SkipReason`, `AuthReasonCode`, and `UiCommand` variants (never expose client-supplied role TTL overrides or manual A|B promotion).
- **View Models & Diffs (`view`)**:
  - Skin-agnostic `ViewModel` (`Clone + PartialEq + Eq`) with fail-closed `AuthState` (`Unresolved` before session resolution, zero invite disclosure on `AuthorizationDenied`, mandatory `0`-Passkey gating).
  - Incremental `diff(old, new) -> Vec<Delta>` and `apply(old, delta)` ensuring minimal DOM updates across all v6 fields.
  - Public log sanitization: Masks private LAN IPv4 addresses and internal node hostnames before UI rendering.
- **REST & SSE DTOs (`dto`)**:
  - 1:1 `mu-auth` wire DTOs re-exported at the crate root (`SubsystemId`, `SubsystemStatus`, `MaintenanceMode`, `MaintenanceOverride`, `RoleSpec`, `PolicyFolio`, `RoleFolioIndex`), 7-subsystem `SubsystemHealthSummary`, ecosystem-agnostic `CdRunDto`, `ModelRunnerStatusDto`, `ModelRunnerConnectionDto`, and typed `paths::*` builders (`/api/auth/*`, `/api/cd/runs`, `/api/cd/deployments/{id}/rollback`, `/api/obs/v1/status/subsystems`, `/api/mr/v1/*`).
  - Keep custom deserializers (e.g., `null_default`) gated on `is_human_readable()` so non-self-describing `postcard` frames remain binary-identical to `#[derive(Serialize)]`.
- **Worker Configuration (`config`)**:
  - Query parameter extraction for `?api=`, `?scenario=`, `?invite=`.

## 3. Privacy & Target Invariants

- **Public Repository Privacy Boundary**:
  - All public issues, documentation, and error traces must use generic placeholders (`node-a`, `worker-riscv64`, `192.0.2.x`).
- **Zero-Dependency Core**:
  - `mu-console-protocol` is pure `#![no_std]` + `alloc`, compiling natively and cleanly targeting `wasm32-unknown-unknown` without pulling browser or DOM dependencies.
