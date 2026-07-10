//! ADGL parser/verifier no-panic fuzz smoke harness.
//!
//! Fast CI smoke:
//! `cargo test -p airpulse_dsl-fuzz`
//!
//! Long manual campaign:
//! `cargo test -p airpulse_dsl-fuzz mutation_seed_corpus_no_panic_long -- --ignored`

#[cfg(test)]
const MAX_ADGL_SOURCE_BYTES: usize = 4 * 1024 * 1024;
#[cfg(test)]
const SMOKE_BYTE_CASES: u32 = 512;
#[cfg(test)]
const SMOKE_STRING_CASES: u32 = 512;
#[cfg(test)]
const SMOKE_MUTATIONS_PER_SEED: usize = 256;
#[cfg(test)]
const LONG_MUTATIONS_PER_SEED: usize = 16_384;
#[cfg(test)]
const MAX_MUTATED_SOURCE_BYTES: usize = 64 * 1024;

#[cfg(test)]
const EXAMPLE_SEEDS: [&str; 10] = [
    include_str!("../../../docs/idea/examples/01-pmtud-blackhole.adgl"),
    include_str!("../../../docs/idea/examples/02-tcp-retrans-seed.adgl"),
    include_str!("../../../docs/idea/examples/03-auth-outage-impact.adgl"),
    include_str!("../../../docs/idea/examples/04-dhcp-missing-auth.adgl"),
    include_str!("../../../docs/idea/examples/05-crc-link-flap.adgl"),
    include_str!("../../../docs/idea/examples/06-link-absent.adgl"),
    include_str!("../../../docs/idea/examples/07-suppress-downstream.adgl"),
    include_str!("../../../docs/idea/examples/08-stp-tcp-burst.adgl"),
    include_str!("../../../docs/idea/examples/09-ap-deauth-missing-rf.adgl"),
    include_str!("../../../docs/idea/examples/10-ambiguity-demo.adgl"),
];

#[cfg(test)]
fn seed_state(seed_idx: usize, iter: usize) -> u64 {
    // Stable one-way mixing to produce deterministic per-case entropy.
    let mut x = (seed_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (iter as u64);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

#[cfg(test)]
fn next_u64(state: &mut u64) -> u64 {
    // xorshift64*
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

#[cfg(test)]
fn bounded_index(state: &mut u64, upper: usize) -> usize {
    if upper == 0 {
        0
    } else {
        (next_u64(state) as usize) % upper
    }
}

#[cfg(test)]
fn mutate_seed_bytes(seed: &[u8], seed_idx: usize, iter: usize) -> Vec<u8> {
    let mut state = seed_state(seed_idx, iter);
    let mut out = seed.to_vec();
    let ops = 1 + bounded_index(&mut state, 8);
    for _ in 0..ops {
        match bounded_index(&mut state, 4) {
            0 => {
                // Flip one random bit.
                if !out.is_empty() {
                    let idx = bounded_index(&mut state, out.len());
                    let bit = 1u8 << bounded_index(&mut state, 8);
                    out[idx] ^= bit;
                }
            }
            1 => {
                // Insert one random byte.
                if out.len() < MAX_MUTATED_SOURCE_BYTES {
                    let idx = bounded_index(&mut state, out.len() + 1);
                    let byte = next_u64(&mut state) as u8;
                    out.insert(idx, byte);
                }
            }
            2 => {
                // Delete one byte.
                if !out.is_empty() {
                    let idx = bounded_index(&mut state, out.len());
                    out.remove(idx);
                }
            }
            _ => {
                // Duplicate a short span at another position.
                if !out.is_empty() && out.len() < MAX_MUTATED_SOURCE_BYTES {
                    let start = bounded_index(&mut state, out.len());
                    let width = 1 + bounded_index(&mut state, 24);
                    let end = (start + width).min(out.len());
                    let chunk = out[start..end].to_vec();
                    let insert_at = bounded_index(&mut state, out.len() + 1);
                    out.splice(insert_at..insert_at, chunk);
                }
            }
        }
    }
    if out.len() > MAX_MUTATED_SOURCE_BYTES {
        out.truncate(MAX_MUTATED_SOURCE_BYTES);
    }
    out
}

#[cfg(test)]
fn parse_then_verify_no_panic(src: &str) {
    if let Ok(ast) = airpulse_dsl_syntax::parse_ruleset(src) {
        let _ = airpulse_dsl_verify::verify(&ast);
    }
}

#[cfg(test)]
fn run_seed_mutation_campaign(iterations_per_seed: usize) {
    for (seed_idx, seed) in EXAMPLE_SEEDS.iter().enumerate() {
        for iter in 0..iterations_per_seed {
            let candidate = mutate_seed_bytes(seed.as_bytes(), seed_idx, iter);
            let source = String::from_utf8_lossy(&candidate);
            let _ = airpulse_dsl_syntax::parse_ruleset(&source);
            parse_then_verify_no_panic(&source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: SMOKE_BYTE_CASES,
            .. ProptestConfig::default()
        })]

        #[test]
        fn parse_ruleset_arbitrary_bytes_no_panic(data in proptest::collection::vec(any::<u8>(), 0..65_536)) {
            let source = String::from_utf8_lossy(&data);
            let _ = airpulse_dsl_syntax::parse_ruleset(&source);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: SMOKE_STRING_CASES,
            .. ProptestConfig::default()
        })]

        #[test]
        fn parse_then_verify_arbitrary_strings_no_panic(chars in proptest::collection::vec(any::<char>(), 0..8_192)) {
            let source: String = chars.into_iter().collect();
            parse_then_verify_no_panic(&source);
        }
    }

    #[test]
    fn parse_ruleset_oversize_input_hits_limit_without_panic() {
        let source = "x".repeat(MAX_ADGL_SOURCE_BYTES + 1);
        let err =
            airpulse_dsl_syntax::parse_ruleset(&source).expect_err("oversize source must fail");
        assert!(
            err.iter().any(|d| d.code == "ADGL0102"),
            "expected ADGL0102 for oversize source"
        );
    }

    #[test]
    fn mutation_seed_corpus_no_panic_smoke() {
        run_seed_mutation_campaign(SMOKE_MUTATIONS_PER_SEED);
    }

    #[test]
    #[ignore = "manual long fuzz run"]
    fn mutation_seed_corpus_no_panic_long() {
        run_seed_mutation_campaign(LONG_MUTATIONS_PER_SEED);
    }
}
