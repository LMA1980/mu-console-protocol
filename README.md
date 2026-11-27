# mu-console-protocol

Zero-dependency (`#![no_std]` + `alloc`, `#![forbid(unsafe_code)]`) IPC view models, REST/SSE DTOs, and versioned postcard codec (`PROTOCOL_VERSION = 10`) connecting the [`mu-console`](https://github.com/LMA1980/mu-console) UI and data worker.

## Architecture

- **Areas, Skins & Dialogs (`model`)**:
  - 8 canonical role-gated `Area` variants (`Dashboard`, `Ci`, `Cd`, `AbDeploy`, `Repository`, `Observability`, `ModelRunner`, `Platform`) spanning 27 `SubView` routes, where `Area::Repository` uses the canonical v8 route slug `"registry"` and label `"Registry"` with legacy hash alias resolution (`#/iam` -> `Platform`, `#/fleet` -> `Ci`, `#/repository` / `#/reg` -> `Repository`, `#/observe` / `#/obs` -> `Observability`, `#/system` / `#/overview` -> `Dashboard`).
  - Dual skin model (`ConsoleSkin::{Html, Wireframe}`, default `Html`) paired with skin-agnostic `DialogModel` (`PasskeyEnrollmentGate`, `UserProfile`, `SessionRoles`, `RiskConfirm`, `InviteCreatedOnce`).
  - Server-authoritative RBAC lease views (`RoleLeaseView`, including renewal window metadata `renew_window_secs`, `renew_extend_secs`, `max_renewals`, `renewals_used`, and `can_renew`), structured `SkipReason` / `AuthReasonCode` tokens, `UiCommand` actions including `RenewRoleWithPasskey` (without client-supplied TTL overrides or manual A|B promotion), and `SessionRoleDescriptor` (`unassigned` catalog state).
- **View Models & Incremental Diffs (`view`)**:
  - `ViewModel` (`Clone + PartialEq + Eq`), `Delta`, `diff`, and `apply` with fail-closed `AuthState` (`Unresolved`, `Anonymous`, `AuthorizationDenied`, `PasskeyEnrollmentRequired`, `Active`), `SubsystemHealthSummary`, `CdRunDto`, `ModelRunnerView`, and `PlatformSnapshotDto`.
  - Sanitized public log redaction masking private IPv4 addresses and internal paths.
- **Wire DTOs & Endpoints (`dto`)**:
  - 1:1 [`mu-auth`](https://github.com/LMA1980/mu-auth) mirrored types (`SubsystemId`, `SubsystemStatus`, `MaintenanceMode`, `MaintenanceOverrideDto`, `StepUpRequirement`, `RoleActivationDto`, `RoleSpecDto`, `PolicyFolioDto`, `RoleFolioIndexDto`), `AuthSessionDto` (`bound_role_specs`, `catalog_role_specs`, `leases`), ecosystem-agnostic `CdRunDto` (Cargo, .NET, Maven, custom), `ModelRunnerStatusDto`, `ModelRunnerConnectionDto`, and canonical `paths::*` constants/builders (`/api/auth/*`, `/api/auth/roles/{role_id}/renew/start`, `/api/auth/roles/{role_id}/renew`, `/api/cd/runs`, `/api/cd/deployments/{id}/rollback`, `/api/obs/v1/status/subsystems`, `/api/obs/v1/ingress/{overview,routes,upstreams}`, `/api/mr/v1/*`).
- **Codec (`codec`)**:
  - Versioned `postcard` framing (`varint(u32 PROTOCOL_VERSION) ++ postcard(body)`) with strict trailing-byte and version-mismatch rejection (`PROTOCOL_VERSION = 10`).

## Ecosystem Integration

- **Parent Workspace**: [`mu-console`](https://github.com/LMA1980/mu-console)
- **Sibling Crates**: [`mu-console-theme`](https://github.com/LMA1980/mu-console-theme), [`mu-console-layout`](https://github.com/LMA1980/mu-console-layout), [`mu-console-data`](https://github.com/LMA1980/mu-console-data), [`mu-console-ui`](https://github.com/LMA1980/mu-console-ui)
- **Identity & Policy Types**: [`mu-auth`](https://github.com/LMA1980/mu-auth)

## Testing & Verification

```bash
cargo fmt --all -- --check
./scripts/format_docs.sh --check
cargo agents plugin validate Symposium.toml --no-check-crates
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --target wasm32-unknown-unknown
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --workspace --lib
```

## License

`mu-console-protocol` is licensed under the GNU Lesser General Public License version 3.0 with the LGPL-3.0 Linking Exception (`LGPL-3.0-only WITH LGPL-3.0-linking-exception`). See [LICENSE](LICENSE), [COPYING](COPYING), and [LINKING-EXCEPTION](LINKING-EXCEPTION).
