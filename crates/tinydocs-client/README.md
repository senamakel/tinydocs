# TinyDocs client

`tinydocs-client` is the host-side typed client for the `tinydocs` TinyBus
module. It translates contract methods and held outputs into typed Rust calls.
It does not load modules, decide readiness, configure reporting, or choose
deadlines; those choices belong to the embedding host.

Ordinary typed calls use `ModuleClient::call_detailed`. Upload operations use
`ModuleClient::call_with_stream_detailed`, which keeps readiness, registry
validation, timeouts, and failure reporting inside the SDK. Held-output reads
use `ModuleObjectClient::call_detailed` so their wire errors retain the same
contract distinctions. The host continues to own module loading and transport
lifecycle.

`DocumentCallError` keeps unavailable, invalid-input, and execution failures
distinct. Its `Debug` and `Display` forms expose only the category. A caller
that needs TinyDocs' input detail can opt into `raw_message()` and apply its own
redaction policy before surfacing that untrusted text.

Variable-size results use the contract's held-output protocol. The client
reads bounded base64 chunks, checks the declared length and SHA-256 digest, and
releases each output after success or failure. On cancellation, it makes a
best-effort release call on the caller's Tokio runtime. If the runtime is
already gone or that call cannot be delivered, the client cannot guarantee
that the module has physically reclaimed the output; module-side expiry remains
the fallback. The contract and behavior are exercised by
`tests/recording_broker.rs`, which serves a local recording interface over
TinyBus's in-memory broker and never contacts a live module.
