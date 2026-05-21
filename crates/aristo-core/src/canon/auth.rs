//! Re-export shim — the real auth module lives at
//! [`aristo_core::auth`](crate::auth).
//!
//! The canon::auth path was the original home for token resolution
//! when the canon API was the only consumer. Auth is now foundational
//! SDK infrastructure (future server-side features will also need it),
//! so the real code lives at [`crate::auth`] and this shim exists so
//! in-flight callers don't break in the same commit that moves the
//! code. New code should import from [`crate::auth`] directly.

pub use crate::auth::*;
