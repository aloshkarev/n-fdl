//! N-FDL Plugin ABI v1 stub (pure Rust).
//!
//! Implements manifest + invoke registry and the reference `dns_decompress`
//! plugin per spec `10-plugin-abi.md`. Full C FFI / HPACK / stateful plugins
//! are intentionally out of scope for this stub (`#![forbid(unsafe_code)]`).

#![forbid(unsafe_code)]
#![warn(clippy::all)]

pub mod abi;
pub mod dns_decompress;
pub mod registry;
pub mod verify;

pub use abi::{
    AbiType, BufView, PluginError, PluginFlags, PluginManifest, PluginStatus, PluginValue, Purity,
    ABI_VERSION,
};
pub use dns_decompress::{dns_decompress, MAX_JUMPS};
pub use registry::{InvokeFn, PluginRegistry, RegisteredPlugin};
pub use verify::{verify_invoke_arity, verify_invoke_name};

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a classic uncompressed DNS name: `www.example.com\0`
    fn uncompressed_www_example_com() -> Vec<u8> {
        let mut buf = Vec::new();
        for label in ["www", "example", "com"] {
            buf.push(label.len() as u8);
            buf.extend_from_slice(label.as_bytes());
        }
        buf.push(0);
        buf
    }

    #[test]
    fn invoke_dns_decompress_uncompressed() {
        let reg = PluginRegistry::with_builtins();
        let buf = uncompressed_www_example_com();
        let out = reg
            .invoke(
                "dns_decompress",
                &buf,
                &[PluginValue::Opaque, PluginValue::U64(0)],
            )
            .expect("invoke");
        assert_eq!(out.field("name").and_then(|v| v.as_str()), Some("www.example.com"));
        assert_eq!(out.field("wire_len").and_then(|v| v.as_u16()), Some(buf.len() as u16));
    }

    #[test]
    fn invoke_dns_decompress_with_pointer() {
        // Layout:
        //   0: example.com\0          (used as compression target)
        //  13: www + pointer(0xC000)  → www.example.com, wire_len = 2+2 = 4
        let mut buf = Vec::new();
        for label in ["example", "com"] {
            buf.push(label.len() as u8);
            buf.extend_from_slice(label.as_bytes());
        }
        buf.push(0); // offset 13 is end of first name
        assert_eq!(buf.len(), 13);

        let www_off = buf.len();
        buf.push(3);
        buf.extend_from_slice(b"www");
        buf.push(0xC0);
        buf.push(0x00); // pointer to offset 0

        let reg = PluginRegistry::with_builtins();
        let out = reg
            .invoke(
                "dns_decompress",
                &buf,
                &[PluginValue::U64(www_off as u64)],
            )
            .expect("invoke");
        assert_eq!(out.field("name").and_then(|v| v.as_str()), Some("www.example.com"));
        // on-wire: len(3) + "www" + 2-byte pointer = 6
        assert_eq!(out.field("wire_len").and_then(|v| v.as_u16()), Some(6));
    }

    #[test]
    fn dns_decompress_detects_pointer_loop() {
        // Self-referential pointer at 0: C0 00
        let buf = vec![0xC0, 0x00];
        let err = dns_decompress(BufView::new(&buf, 0)).expect_err("loop");
        assert_eq!(err.status, PluginStatus::Limit);
    }

    #[test]
    fn dns_decompress_max_jumps() {
        // Chain of pointers longer than MAX_JUMPS: each points to next 2-byte pointer.
        // Build MAX_JUMPS+2 pointer cells at even offsets.
        let n = (MAX_JUMPS as usize) + 2;
        let mut buf = vec![0u8; n * 2];
        for i in 0..n {
            let off = i * 2;
            let target = ((i + 1) * 2) as u16;
            buf[off] = 0xC0 | ((target >> 8) as u8);
            buf[off + 1] = (target & 0xFF) as u8;
        }
        // Last cell points to itself to force eventual loop if jumps weren't capped —
        // but MAX_JUMPS should fire first.
        let last = (n - 1) * 2;
        buf[last] = 0xC0;
        buf[last + 1] = 0x00;

        let err = dns_decompress(BufView::new(&buf, 0)).expect_err("MAX_JUMPS");
        assert_eq!(err.status, PluginStatus::Limit);
        assert!(err.message.contains("MAX_JUMPS"), "{}", err.message);
    }

    #[test]
    fn dns_decompress_malformed_truncated_label() {
        let buf = vec![5, b'a', b'b']; // claims len 5, only 2 bytes follow
        let err = dns_decompress(BufView::new(&buf, 0)).expect_err("truncated");
        assert_eq!(err.status, PluginStatus::Malformed);
    }

    #[test]
    fn verify_unknown_plugin() {
        let reg = PluginRegistry::with_builtins();
        assert!(verify_invoke_name(&reg, "dns_decompress").is_ok());
        let err = verify_invoke_name(&reg, "no_such_plugin").unwrap_err();
        assert!(err.message.contains("unknown plugin"));
    }

    #[test]
    fn verify_arity_matches_manifest() {
        let reg = PluginRegistry::with_builtins();
        assert!(verify_invoke_arity(&reg, "dns_decompress", 2).is_ok());
        assert!(verify_invoke_arity(&reg, "dns_decompress", 1).is_err());
    }

    #[test]
    fn unknown_invoke_name_errors() {
        let reg = PluginRegistry::with_builtins();
        let err = reg.invoke("hpack", &[], &[]).unwrap_err();
        assert!(err.message.contains("unknown plugin"));
    }
}
