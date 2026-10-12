# Roadmap

TinyDocs generates and reads documents through a Rust API and a TinyBus native
module. This roadmap tracks shipped capabilities and the next document-transfer
and format work.

## Shipped

- module layout, crate-wide error type, and the public re-export surface
- lint configuration in `[lints]`, enforced identically locally and in CI
- CI: format, clippy, build, test, rustdoc, MSRV, and supply-chain checks
- a shared release workflow triggered by tags on reviewed version commits
- a TinyBus-loadable native module exposing DOCX generation
- installable Linux and macOS bundles with the TinyBus host, module allowlist,
  source packages, and protocol documentation on GitHub releases
- end-to-end coverage through TinyBus's real dynamic loader and broker

## Next

- path or file-descriptor transfer for document formats that outgrow the bus
  frame limit
- additional document formats behind focused feature flags

## Out Of Scope

- anything that cannot be tested deterministically
- convenience wrappers that hide the crate's error taxonomy from callers
