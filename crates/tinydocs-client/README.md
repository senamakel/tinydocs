# TinyDocs client

`tinydocs-client` is the host-side typed client for the `tinydocs` TinyBus
module. It translates contract methods and held outputs into typed Rust calls.
It does not load modules, decide readiness, configure reporting, or choose
deadlines; those choices belong to the embedding host.

Ordinary typed calls use `ModuleClient::call_detailed`. The existing adapter
also needs streamed input, which the published TinyBus SDK does not currently
expose through `ModuleClient`. The client contract tests specify the required
SDK addition: a detailed streaming call that retains module readiness,
registry-address validation, the configured timeout, and failure reporting.
The host continues to own module loading and transport lifecycle.

Variable-size results use the contract's held-output protocol. The client
reads bounded base64 chunks, checks the declared length and SHA-256 digest, and
releases each output after success or failure. Cancellation schedules the
release through the caller's Tokio runtime. The contract and behavior are
exercised by `tests/recording_broker.rs`, which serves a local recording
interface over TinyBus's in-memory broker and never contacts a live module.
