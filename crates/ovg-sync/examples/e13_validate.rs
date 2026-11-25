use ovg_sync::field::validate_offline_ops;
use serde_json::Value;
use std::{hint::black_box, io::Read, time::Instant};
fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let v: Value = serde_json::from_str(&input).unwrap();
    let ops = v["ops"].as_array().unwrap();
    let runs = v["runs"].as_u64().unwrap_or(1);
    let mut times = Vec::new();
    let mut errors = 0;
    for _ in 0..runs {
        let t = Instant::now();
        if black_box(validate_offline_ops(&v["snapshot"], ops)).is_err() { errors += 1; }
        times.push(t.elapsed().as_nanos());
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let rss: u64 = status.lines().find(|l| l.starts_with("VmHWM:")).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
    println!("{}", serde_json::json!({"samples_ns": times, "errors": errors, "peak_rss_kib": rss}));
}
