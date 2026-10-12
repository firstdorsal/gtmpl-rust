//! Custom mows template functions
//!
//! These are additional functions specific to mows.
//! Enable with the `mows-functions` feature flag.

pub mod crypto;
pub mod utils;

use crate::Func;

/// Custom mows template functions
///
/// These are additional functions specific to mows.
pub const MOWS_FUNCTIONS: [(&str, Func); 3] = [
    ("mowsRandomString", crypto::random_string as Func),
    ("mowsDigest", crypto::mows_digest as Func),
    ("mowsJoinDomain", utils::join_domain as Func),
];
