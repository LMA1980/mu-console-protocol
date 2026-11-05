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

- **Commands & Events (`model`)**:
  - `UiCommand`: Commands dispatched from UI to data worker (e.g., `Configure`, `Navigate`, `SelectRepo`, `Filter`).
  - `DataEvent`: State updates and telemetry streamed from data worker to UI (e.g., `Snapshot`, `Patch`, `AuthSession`, `WorkerReady`).
- **View Models & Diffs (`view`)**:
  - Skin-agnostic `ViewModel` representation.
  - Incremental `diff(old, new) -> Delta` and `apply(old, delta) -> ViewModel` ensuring minimal DOM updates.
  - Public log sanitization: Masks private LAN IPv4 addresses and internal node hostnames before UI rendering.
- **REST & SSE DTOs (`dto`)**:
  - Strongly typed serde mappings for Axum backend endpoints (`/api/repos`, `/api/events`, `/api/cd`, `/api/obs`).
- **Worker Configuration (`config`)**:
  - Query parameter extraction for `?api=`, `?scenario=`, `?invite=`.

## 3. Privacy & Target Invariants

- **Public Repository Privacy Boundary**:
  - All public issues, documentation, and error traces must use generic placeholders (`node-a`, `worker-riscv64`, `192.0.2.x`).
- **Zero-Dependency Core**:
  - `mu-console-protocol` is pure `#![no_std]` + `alloc`, compiling natively and cleanly targeting `wasm32-unknown-unknown` without pulling browser or DOM dependencies.
