//! Combined template functions module
//!
//! This module provides all template functions (Helm + mows) combined.
//! Enable with the `all-functions` feature flag.

use crate::helm_functions::HELM_FUNCTIONS;
use crate::mows_functions::MOWS_FUNCTIONS;
use crate::Func;

/// Get all template functions (Helm + mows) as a Vec
///
/// This function returns all 155 template functions:
/// - 152 Helm-compatible functions
/// - 3 mows-specific functions
pub fn all_functions() -> Vec<(&'static str, Func)> {
    let mut funcs = Vec::with_capacity(HELM_FUNCTIONS.len() + MOWS_FUNCTIONS.len());
    funcs.extend_from_slice(&HELM_FUNCTIONS);
    funcs.extend_from_slice(&MOWS_FUNCTIONS);
    funcs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_functions_count() {
        let funcs = all_functions();
        assert_eq!(funcs.len(), 155); // 152 helm + 3 mows
    }
}
