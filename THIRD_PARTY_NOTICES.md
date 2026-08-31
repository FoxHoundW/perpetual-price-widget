# Third-party notices

This project is built with open-source dependencies distributed through Cargo and npm/pnpm. Dependency versions are recorded in `src-tauri/Cargo.lock` and `pnpm-lock.yaml`; the corresponding license texts and notices are available from each upstream package distribution.

Principal runtime components include:

- Tauri 2 — Apache-2.0 / MIT.
- Rust standard library and Tokio — Apache-2.0 / MIT.
- reqwest, tokio-tungstenite and futures-util — Apache-2.0 / MIT.
- async-http-proxy, tokio-socks and sysproxy — MIT.
- keyring-rs — Apache-2.0 / MIT; used only to access Windows Credential Manager.
- rusqlite and SQLite — MIT / public domain, respectively.
- Vite and TypeScript — MIT / Apache-2.0.

No source code from the GPL-3.0 or AGPL-3.0 reference projects listed in the product specification is included in this repository.

Binance names and market data remain subject to Binance's own terms. This project is not affiliated with or endorsed by Binance.

This notice is provided for attribution and does not replace the license terms distributed by the individual upstream projects.
