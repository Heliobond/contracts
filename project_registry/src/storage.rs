//! Storage-accessor helpers.
//!
//! Historically this module exposed read_project / write_project /
//! read_proposal / write_proposal / read_whitelist / write_whitelist wrappers
//! around env.storage(), but lib.rs inlines env.storage() calls directly and
//! never referenced them. They were removed to eliminate dead, drift-prone
//! abstractions (#331).
