//! Pomiar RSS procesu w kolejnych etapach: start, po `load_model`, po syntezach, po `unload` (tylko Linux).
//!
//!     cargo run --release -p plkokoro --example memory
//!
//! Wymaga tego samego co `basic`. Uwaga: `unload` zwalnia sesję ONNX, ale `libonnxruntime.so` zostaje zmapowana do
//! końca procesu (środowisko ORT w crate `ort` jest globalne), więc RSS nie spada do wartości sprzed ładowania.

use plkokoro::{load_model, Config, SynthOptions, TextOptions};

fn rss_mb() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kb = status.lines().find_map(|l| l.strip_prefix("VmRSS:")).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok());
    kb.unwrap_or(f64::NAN) / 1024.0
}

fn main() -> plkokoro::Result<()> {
    println!("{:<26} RSS = {:6.1} MB", "start", rss_mb());
    let model = load_model(Config::default())?;
    println!("{:<26} RSS = {:6.1} MB", "po load_model", rss_mb());
    for i in 1..=3 {
        let ipa = model.text_to_ipa("Cześć, to jest test polskiej syntezy mowy.", &TextOptions::default())?;
        model.synthesize(&ipa, &SynthOptions::default())?;
        println!("{:<26} RSS = {:6.1} MB", format!("po synthesize x{i}"), rss_mb());
    }
    model.unload()?;
    println!("{:<26} RSS = {:6.1} MB", "po unload", rss_mb());
    Ok(())
}
