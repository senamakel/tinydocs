//! Typed host client for the `tinydocs` TinyBus module.
//!
//! Module loading, readiness, host policy, and transport ownership stay with
//! the embedding host. The client accepts that host's `ModuleClient`, then
//! owns TinyDocs contract calls and held-output transfer.
