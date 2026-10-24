/**
 * The IPC contract, shared by the webview, the sidecar and (through `contract/fixtures/`) the Rust
 * core.
 *
 * One module per domain. Adding a domain is one `export *` line here — the seam that keeps parallel
 * tickets from fighting over a single file.
 */

export * from "./core";
export * from "./git";
export * from "./skills";
export * from "./specs";
