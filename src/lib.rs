//! The wire of the Grid: the protocol library that Master Control and every TronGrid Lite
//! instance load as the same shared binary, so the two ends of the wire cannot disagree about
//! what travels over it.
//!
//! The wire's design authority — messages, framing, handshake, trust — is `docs/TOPOLOGY.md` in
//! the `tron-grid-lite` repository. Design changes land there before code lands here.
