//! Binding symbols per `docs/idea/spec/06-ir-bytecode.md` §2.1.

/// A binding name introduced by an `anchor` or `correlate` clause, e.g.
/// `"rtx"`, `"ptb"`, `"upstream"` (`06-ir-bytecode.md` §2.1 `AnchorSpec.binding`
/// / `CorrelateSpec.binding`; name resolution rules in `03-semantics.md` §5).
///
/// Symbolic (owned string) at the IR level; the hot-path opcodes reference
/// bindings by [`crate::BindingIdx`] instead (`06` §6 zero-copy).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(Box<str>);

impl Symbol {
    /// Wraps a binding name.
    #[must_use]
    pub fn new(name: impl Into<Box<str>>) -> Self {
        Self(name.into())
    }

    /// The binding name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Symbol {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_value_semantics() {
        assert_eq!(Symbol::new("rtx"), Symbol::from("rtx"));
        assert_ne!(Symbol::new("rtx"), Symbol::new("ptb"));
        assert_eq!(Symbol::new("upstream").as_str(), "upstream");
    }
}
