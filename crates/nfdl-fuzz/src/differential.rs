//! Production Differential Harness (v1 progress)
//! Now computes a basic match rate against "golden" (tshark or hardcoded).

use std::process::Command;

pub fn run_nfdl_cli(file: &str) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["run", "-p", "nfdl-cli", "--", file])
        .current_dir("..")
        .output()
        .map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn run_tshark(pcap: &str) -> Result<String, String> {
    let output = Command::new("tshark")
        .args(["-r", pcap, "-T", "json"])
        .output();
    
    match output {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
        Err(e) => Err(format!("TShark unavailable: {}", e)),
    }
}

pub fn normalize_and_compare(nfdl_out: &str, golden: &str) -> (bool, f64, String) {
    // Simple production metric: word overlap + exact
    let nfdl_words: Vec<_> = nfdl_out.split_whitespace().collect();
    let gold_words: Vec<_> = golden.split_whitespace().collect();
    
    let common = nfdl_words.iter().filter(|w| gold_words.contains(w)).count();
    let rate = if gold_words.is_empty() { 0.0 } else { common as f64 / gold_words.len() as f64 * 100.0 };
    
    let exact = nfdl_out.trim() == golden.trim();
    let status = if exact { "EXACT MATCH".into() } else if rate > 70.0 { format!("PARTIAL {:.1}%", rate) } else { "DIFF".into() };
    (exact, rate, status)
}

pub fn differential_match_rate(nfdl_file: &str, golden_summary: &str) -> f64 {
    if let Ok(nfdl) = run_nfdl_cli(nfdl_file) {
        let (_, rate, _) = normalize_and_compare(&nfdl, golden_summary);
        rate
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differential_nfdl_cli() {
        let result = run_nfdl_cli("../../../docs/examples/arp.nfdl");
        assert!(result.is_ok());
    }

    #[test]
    fn differential_match_rate_demo() {
        // Simulate golden for ARP
        let golden = "N-FDL v1 SUCCESS Protocol: ARP Messages: 1";
        let rate = differential_match_rate("../../../docs/examples/arp.nfdl", golden);
        println!("Differential match rate for arp: {:.1}%", rate);
        assert!(rate > 0.0); // at least runs
    }
}
