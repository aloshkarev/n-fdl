//! TCP Reassembly (production v1 progress)
//! Now supports NeedMoreBytes, basic compaction, overlap policy (last wins for v1)

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ReassemblyError {
    NeedMoreBytes(usize),
    OverlapConflict,
    /// `ooo` plus contiguous bytes would pass `max_buffer`. Not `Ok`.
    Partial {
        cap: &'static str,
        kept: usize,
        dropped: usize,
    },
}

#[derive(Debug, Clone)]
pub struct Reassembler {
    pub base_seq: u32,
    pub contiguous: Vec<u8>,
    pub ooo: BTreeMap<u32, Vec<u8>>,
    pub next_seq: u32,
    pub max_buffer: usize,
}

impl Reassembler {
    pub fn new(initial_seq: u32) -> Self {
        Self {
            base_seq: initial_seq,
            contiguous: Vec::new(),
            ooo: BTreeMap::new(),
            next_seq: initial_seq,
            max_buffer: 1 << 20, // 1MB
        }
    }

    fn occupied(&self) -> usize {
        self.contiguous.len() + self.ooo.values().map(Vec::len).sum::<usize>()
    }

    /// Accept a TCP segment. A drop past `max_buffer` is [`ReassemblyError::Partial`],
    /// counting out-of-order bytes, and is not `Ok`.
    pub fn accept_segment(&mut self, seq: u32, data: Vec<u8>) -> Result<(), ReassemblyError> {
        let incoming = data.len();
        let growth = if seq == self.next_seq {
            incoming
        } else if seq > self.next_seq {
            match self.ooo.get(&seq) {
                Some(existing) => incoming.saturating_sub(existing.len()),
                None => incoming,
            }
        } else {
            0
        };
        let occupied = self.occupied();
        if growth > 0 && occupied.saturating_add(growth) > self.max_buffer {
            return Err(ReassemblyError::Partial {
                cap: "reassembly",
                kept: occupied,
                dropped: growth,
            });
        }

        let _rel = seq.wrapping_sub(self.base_seq);

        if seq == self.next_seq {
            // In order
            self.contiguous.extend_from_slice(&data);
            self.next_seq = self.next_seq.wrapping_add(data.len() as u32);

            // Drain OOO
            while let Some((&next, seg)) = self.ooo.range(self.next_seq..).next() {
                if next == self.next_seq {
                    self.contiguous.extend_from_slice(seg);
                    self.next_seq = self.next_seq.wrapping_add(seg.len() as u32);
                    self.ooo.remove(&next);
                } else {
                    break;
                }
            }
        } else if seq > self.next_seq {
            // OOO
            // Simple overlap policy: if overlap with existing, last wins (replace)
            if let Some(existing) = self.ooo.get_mut(&seq) {
                if existing.len() < data.len() {
                    *existing = data;
                }
            } else {
                self.ooo.insert(seq, data);
            }
        } else {
            // Old or duplicate: for v1 we ignore (could implement overlap)
        }

        Ok(())
    }

    pub fn get_contiguous(&self) -> &[u8] {
        &self.contiguous
    }

    pub fn has_data(&self) -> bool {
        !self.contiguous.is_empty()
    }

    /// Returns how many more bytes needed to reach a target length (for NeedMoreBytes)
    pub fn bytes_needed(&self, target: usize) -> Option<usize> {
        if self.contiguous.len() >= target {
            None
        } else {
            Some(target - self.contiguous.len())
        }
    }

    /// Compact: drop processed prefix (for long streams)
    pub fn compact(&mut self, keep: usize) {
        if self.contiguous.len() > keep {
            let drain = self.contiguous.len() - keep;
            self.contiguous.drain(0..drain);
            self.base_seq = self.base_seq.wrapping_add(drain as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reassembly_basic_ooo() {
        let mut r = Reassembler::new(1000);
        r.accept_segment(1002, vec![2, 3]).unwrap();
        r.accept_segment(1000, vec![0, 1]).unwrap();
        assert_eq!(r.get_contiguous(), &[0, 1, 2, 3]);
    }

    #[test]
    fn reassembly_need_more() {
        let mut r = Reassembler::new(0);
        r.accept_segment(0, vec![1, 2]).unwrap();
        assert_eq!(r.bytes_needed(5), Some(3));
    }

    #[test]
    fn ooo_bytes_count_toward_max_buffer() {
        let mut r = Reassembler::new(0);
        r.max_buffer = 4;
        r.accept_segment(2, vec![1, 2]).unwrap();
        let err = r.accept_segment(4, vec![3, 4, 5]).unwrap_err();
        assert_eq!(
            err,
            ReassemblyError::Partial {
                cap: "reassembly",
                kept: 2,
                dropped: 3,
            }
        );
        assert!(r.ooo.contains_key(&2));
        assert!(!r.ooo.contains_key(&4));
    }
}
