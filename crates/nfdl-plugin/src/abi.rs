//! Plugin ABI v1 types (spec `10-plugin-abi.md`).
//!
//! Pure-Rust stub of the C ABI; no `extern "C"` / FFI yet.

/// Current Plugin ABI major version for this stub.
pub const ABI_VERSION: u32 = 1;

/// Purity class of a plugin (v1 PureStateless; Stateful reserved for v1.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purity {
    PureStateless,
    Stateful,
}

/// Static ABI type used in manifests and (minimal) signature checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    Bool,
    Str,
    Bytes,
    Opaque,
    Record(Vec<(String, AbiType)>),
}

/// Capability flags from the plugin manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PluginFlags {
    pub may_read_root: bool,
    pub needs_root_offset: bool,
}

impl PluginFlags {
    pub const MAY_READ_ROOT: Self = Self {
        may_read_root: true,
        needs_root_offset: false,
    };

    pub const NEEDS_ROOT_OFFSET: Self = Self {
        may_read_root: false,
        needs_root_offset: true,
    };

    pub const fn union(self, other: Self) -> Self {
        Self {
            may_read_root: self.may_read_root || other.may_read_root,
            needs_root_offset: self.needs_root_offset || other.needs_root_offset,
        }
    }
}

/// Declared at registration; compiler/verify read this for typecheck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub abi_version: u32,
    pub name: &'static str,
    pub purity: Purity,
    pub args: Vec<AbiType>,
    pub ret: AbiType,
    pub flags: PluginFlags,
}

/// Status codes mirroring `nfdl_status` (spec §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    Ok = 0,
    Malformed = 1,
    Limit = 2,
    Internal = 3,
    NeedMore = 4,
}

/// Tagged value across the (Rust) invoke boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I64(i64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    Opaque,
    Record(Vec<(String, PluginValue)>),
}

impl PluginValue {
    /// Look up a named field in a [`PluginValue::Record`].
    pub fn field(&self, name: &str) -> Option<&PluginValue> {
        match self {
            PluginValue::Record(fields) => {
                fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            PluginValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_u16(&self) -> Option<u16> {
        match self {
            PluginValue::U16(v) => Some(*v),
            PluginValue::U64(v) => u16::try_from(*v).ok(),
            _ => None,
        }
    }
}

/// Read-only view into the root packet buffer (spec `nfdl_buf_view`).
#[derive(Debug, Clone, Copy)]
pub struct BufView<'a> {
    pub data: &'a [u8],
    /// Absolute offset into `data` (C8 / `__root_offset`).
    pub offset: usize,
}

impl<'a> BufView<'a> {
    pub fn new(data: &'a [u8], offset: usize) -> Self {
        Self { data, offset }
    }
}

/// Error returned by invoke / plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginError {
    pub status: PluginStatus,
    pub message: String,
}

impl PluginError {
    pub fn malformed(msg: impl Into<String>) -> Self {
        Self {
            status: PluginStatus::Malformed,
            message: msg.into(),
        }
    }

    pub fn limit(msg: impl Into<String>) -> Self {
        Self {
            status: PluginStatus::Limit,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: PluginStatus::Internal,
            message: msg.into(),
        }
    }

    pub fn unknown_plugin(name: &str) -> Self {
        Self {
            status: PluginStatus::Internal,
            message: format!("unknown plugin: {name}"),
        }
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.status, self.message)
    }
}

impl std::error::Error for PluginError {}
