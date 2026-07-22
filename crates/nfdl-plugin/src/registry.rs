//! Plugin registry and `invoke` dispatch (spec `10-plugin-abi.md` §9).

use std::collections::HashMap;

use crate::abi::{PluginError, PluginManifest, PluginValue};
use crate::dns_decompress;

/// Pure-stateless invoke function: root buffer + tagged args → tagged result.
pub type InvokeFn = fn(&[u8], &[PluginValue]) -> Result<PluginValue, PluginError>;

/// A registered plugin: manifest + late-bound invoke function.
#[derive(Clone)]
pub struct RegisteredPlugin {
    pub manifest: PluginManifest,
    pub invoke: InvokeFn,
}

/// Name → plugin map for the current process (v1 trusted in-process).
#[derive(Clone, Default)]
pub struct PluginRegistry {
    by_name: HashMap<&'static str, RegisteredPlugin>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry preloaded with the reference pure plugins shipped in this crate.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register(RegisteredPlugin {
            manifest: dns_decompress::manifest(),
            invoke: dns_decompress::invoke,
        });
        reg
    }

    pub fn register(&mut self, plugin: RegisteredPlugin) {
        self.by_name.insert(plugin.manifest.name, plugin);
    }

    pub fn get(&self, name: &str) -> Option<&RegisteredPlugin> {
        self.by_name.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn manifests(&self) -> impl Iterator<Item = &PluginManifest> {
        self.by_name.values().map(|p| &p.manifest)
    }

    /// Look up `name` and call its invoke function (Rust-side `nfdl_invoke` stub).
    pub fn invoke(
        &self,
        name: &str,
        root: &[u8],
        args: &[PluginValue],
    ) -> Result<PluginValue, PluginError> {
        let plugin = self
            .get(name)
            .ok_or_else(|| PluginError::unknown_plugin(name))?;
        (plugin.invoke)(root, args)
    }
}
