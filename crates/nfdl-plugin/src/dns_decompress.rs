//! Reference plugin: DNS name decompression (spec `10-plugin-abi.md` §7).
//!
//! Guards: bounds checks, `MAX_JUMPS`, and a visited-set against compression loops.

use crate::abi::{
    ABI_VERSION, AbiType, BufView, PluginError, PluginFlags, PluginManifest, PluginValue, Purity,
};

/// Anti-DoS cap on compression-pointer hops (spec §7).
pub const MAX_JUMPS: u32 = 16;

/// Manifest for `dns_decompress`.
pub fn manifest() -> PluginManifest {
    PluginManifest {
        abi_version: ABI_VERSION,
        name: "dns_decompress",
        purity: Purity::PureStateless,
        args: vec![AbiType::Opaque, AbiType::U64],
        ret: AbiType::Record(vec![
            ("name".into(), AbiType::Str),
            ("wire_len".into(), AbiType::U16),
        ]),
        flags: PluginFlags::MAY_READ_ROOT.union(PluginFlags::NEEDS_ROOT_OFFSET),
    }
}

/// Decompress a DNS domain name starting at `root.offset`.
///
/// Returns `record{ name: str, wire_len: u16 }` where `wire_len` is the on-wire
/// length at the original position (fixed at the first pointer jump, or at the
/// terminating root label if uncompressed).
pub fn dns_decompress(root: BufView<'_>) -> Result<PluginValue, PluginError> {
    let data = root.data;
    let start = root.offset;
    if start > data.len() {
        return Err(PluginError::malformed(format!(
            "root_offset {start} past end of buffer ({})",
            data.len()
        )));
    }

    let mut cur = start;
    let mut jumps: u32 = 0;
    let mut visited = Vec::new();
    let mut name_parts: Vec<String> = Vec::new();
    let mut wire_len: Option<u16> = None;

    loop {
        if cur >= data.len() {
            return Err(PluginError::malformed(format!(
                "DNS name cursor {cur} past end of buffer ({})",
                data.len()
            )));
        }

        let b = data[cur];
        if b == 0 {
            // Root label — terminate.
            if wire_len.is_none() {
                let len = (cur + 1).saturating_sub(start);
                wire_len =
                    Some(u16::try_from(len).map_err(|_| {
                        PluginError::malformed(format!("wire_len {len} exceeds u16"))
                    })?);
            }
            break;
        }

        if (b & 0xC0) == 0xC0 {
            // Compression pointer (2 bytes).
            if cur + 1 >= data.len() {
                return Err(PluginError::malformed("truncated DNS compression pointer"));
            }
            jumps += 1;
            if jumps > MAX_JUMPS {
                return Err(PluginError::limit(format!(
                    "DNS compression jumps exceed MAX_JUMPS ({MAX_JUMPS})"
                )));
            }
            let target = (((b & 0x3F) as usize) << 8) | (data[cur + 1] as usize);
            if visited.contains(&target) {
                return Err(PluginError::limit(format!(
                    "DNS compression pointer loop at target {target}"
                )));
            }
            visited.push(target);
            if wire_len.is_none() {
                let len = (cur + 2).saturating_sub(start);
                wire_len =
                    Some(u16::try_from(len).map_err(|_| {
                        PluginError::malformed(format!("wire_len {len} exceeds u16"))
                    })?);
            }
            cur = target;
            continue;
        }

        // Ordinary label of length `b`.
        let label_len = b as usize;
        let label_end = cur
            .checked_add(1)
            .and_then(|s| s.checked_add(label_len))
            .ok_or_else(|| PluginError::malformed("label length overflow"))?;
        if label_end > data.len() {
            return Err(PluginError::malformed(format!(
                "DNS label overflows buffer: need {label_end}, have {}",
                data.len()
            )));
        }
        let label = &data[cur + 1..label_end];
        // DNS labels are historically LDH; accept raw octets as Latin-1-ish UTF-8 lossy.
        name_parts.push(String::from_utf8_lossy(label).into_owned());
        cur = label_end;
    }

    let name = name_parts.join(".");
    let wire_len = wire_len.unwrap_or(0);

    Ok(PluginValue::Record(vec![
        ("name".into(), PluginValue::Str(name)),
        ("wire_len".into(), PluginValue::U16(wire_len)),
    ]))
}

/// Invoke entry used by the registry: expects `args == [Opaque, U64(offset)]`
/// (or just `[U64(offset)]` with root supplied via `BufView`).
pub fn invoke(root: &[u8], args: &[PluginValue]) -> Result<PluginValue, PluginError> {
    let offset = match args {
        [PluginValue::Opaque, PluginValue::U64(off)] => *off,
        [PluginValue::U64(off)] => *off,
        _ => {
            return Err(PluginError::malformed(
                "dns_decompress expects args [Opaque, U64] or [U64]",
            ));
        }
    };
    let offset = usize::try_from(offset)
        .map_err(|_| PluginError::malformed(format!("root_offset {offset} does not fit usize")))?;
    dns_decompress(BufView::new(root, offset))
}
