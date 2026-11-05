# mu-console-protocol

IPC view models and versioned postcard codec connecting the MuConsole UI and data worker.

## Architecture

- `UiCommand`: Commands dispatched from UI to data worker.
- `DataEvent`: State updates and telemetry streamed from data worker to UI.
- Postcard serialization with framing validation.

## License

`mu-console-protocol` is licensed under the GNU Lesser General Public License version 3.0 with the LGPL-3.0 Linking Exception (`LGPL-3.0-only WITH LGPL-3.0-linking-exception`). See [LICENSE](LICENSE), [COPYING](COPYING), and [LINKING-EXCEPTION](LINKING-EXCEPTION).
