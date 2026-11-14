# mu-console-protocol

Zero-dependency (`#![no_std]` + `alloc`, `#![forbid(unsafe_code)]`) IPC view models, REST/SSE DTOs, and versioned postcard codec (`PROTOCOL_VERSION = 6`) connecting the MuConsole UI and data worker.

## Architecture

- **Areas, Skins & Dialogs (`model`)**:
  - 8 canonical role-gated `Area` variants (`Dashboard`, `Ci`, `Cd`, `AbDeploy`, `Repository`, `Observability`, `ModelRunner`, `Platform`) spanning 27 `SubView` routes, with legacy hash alias resolution (`#/iam` -> `Platform`, `#/fleet` -> `Ci`, `#/cd/gate` -> `AbDeploy`, `#/registry` -> `Repository`, `#/observe` -> `Observability`).
  - Dual skin model (`ConsoleSkin::{Html, Wireframe}`, default `Html`) paired with skin-agnostic `DialogModel` (`PasskeyEnrollmentGate`, `UserProfile`, `SessionRoles`, `RiskConfirm`, `InviteCreatedOnce`).
  - Server-authoritative RBAC lease views (`RoleLeaseView`), structured `SkipReason` / `AuthReasonCode` tokens, `UiCommand` actions (without client-supplied TTL overrides or manual A|B promotion).
- **View Models & Incremental Diffs (`view`)**:
  - `ViewModel` (`Clone + PartialEq + Eq`), `Delta`, `diff`, and `apply` with fail-closed `AuthState` (`Unresolved`, `Anonymous`, `AuthorizationDenied`, `PasskeyEnrollmentRequired`, `Active`), `SubsystemHealthSummary`, `CdRunDto`, `ModelRunnerView`, and `PlatformSnapshotDto`.
  - Sanitized public log redaction masking private IPv4 addresses and internal paths.
- **Wire DTOs & Endpoints (`dto`)**:
  - 1:1 `mu-auth` mirrored types (`SubsystemId`, `SubsystemStatus`, `MaintenanceMode`, `MaintenanceOverrideDto`, `StepUpRequirement`, `RoleActivationDto`, `RoleSpecDto`, `PolicyFolioDto`, `RoleFolioIndexDto`), ecosystem-agnostic `CdRunDto` (Cargo, .NET, Maven, custom), `ModelRunnerStatusDto`, `ModelRunnerConnectionDto`, and canonical `paths::*` constants/builders (`/api/auth/*`, `/api/cd/runs`, `/api/cd/deployments/{id}/rollback`, `/api/obs/v1/status/subsystems`, `/api/mr/v1/*`).
- **Codec (`codec`)**:
  - Versioned `postcard` framing (`varint(u32 PROTOCOL_VERSION) ++ postcard(body)`) with strict trailing-byte and version-mismatch rejection.

## License

`mu-console-protocol` is licensed under the GNU Lesser General Public License version 3.0 with the LGPL-3.0 Linking Exception (`LGPL-3.0-only WITH LGPL-3.0-linking-exception`). See [LICENSE](LICENSE), [COPYING](COPYING), and [LINKING-EXCEPTION](LINKING-EXCEPTION).
