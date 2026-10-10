//! Offline decoder test only. Live requests belong to the verified Devlish runtime.
use devlish_llm::jev::{self, Batch};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return Err("This example only decodes a fixture. Live JEV calls require a verified Devlish program and approved model route.".into());
    }
    let batch: Batch = serde_json::from_str(include_str!("fixtures/jev_batch.json"))?;
    eprintln!("Offline synthetic fixture. No provider request or checklist action.");
    let result = jev::decode(&batch, include_bytes!("fixtures/jev_response.json"), None)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
