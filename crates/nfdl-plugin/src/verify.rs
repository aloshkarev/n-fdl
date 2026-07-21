//! Minimal verify stub: invoke name must exist in the registry (spec §5).
//!
//! Full arity/type/purity checks are deferred; this catches `UnknownPlugin`.

use crate::abi::PluginError;
use crate::registry::PluginRegistry;

/// Ensure `name` is registered. Signature typecheck is intentionally minimal for v1 stub.
pub fn verify_invoke_name(registry: &PluginRegistry, name: &str) -> Result<(), PluginError> {
    if registry.contains(name) {
        Ok(())
    } else {
        Err(PluginError::unknown_plugin(name))
    }
}

/// Optional arity check against the registered manifest (still no type lattice).
pub fn verify_invoke_arity(
    registry: &PluginRegistry,
    name: &str,
    argc: usize,
) -> Result<(), PluginError> {
    let plugin = registry
        .get(name)
        .ok_or_else(|| PluginError::unknown_plugin(name))?;
    let expected = plugin.manifest.args.len();
    if argc == expected {
        Ok(())
    } else {
        Err(PluginError::malformed(format!(
            "plugin `{name}` arity mismatch: expected {expected} args, got {argc}"
        )))
    }
}
