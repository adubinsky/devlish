//! Synthetic data only. Offline by default; --live uses JEV_API_KEY.
use devlish_llm::{
    jev::{self, Batch},
    EnvCredentials,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a != "--live") || args.len() > 1 {
        return Err("usage: jev_document_batch [--live]".into());
    }
    let batch: Batch = serde_json::from_str(include_str!("fixtures/jev_batch.json"))?;
    let result = if args.first().is_some_and(|a| a == "--live") {
        jev::evaluate(&batch, &EnvCredentials, &mut |event| eprintln!("{event}"))?
    } else {
        eprintln!("Offline synthetic fixture. No provider request or checklist action.");
        jev::decode(&batch, include_bytes!("fixtures/jev_response.json"), None)?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
