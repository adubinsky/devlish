use serde_json::{json, Value};

#[test]
fn fractional_evidence_preserves_bits_and_serialized_commitments() {
    // Fixed timestamp-sized values, not the live clock. Nearby IEEE-754 values
    // exercise decimal conversions which the default fast parser can round.
    let base = 1_791_110_000.0f64.to_bits();
    let values = (0..4096)
        .map(|offset| f64::from_bits(base + offset))
        .chain([
            -0.0,
            f64::MIN_POSITIVE,
            f64::MAX,
            1e-300,
            -1.2345678901234567,
        ]);
    for value in values {
        let evidence = json!({"result":value});
        let committed = serde_json::to_vec(&evidence).unwrap();
        let read: Value = serde_json::from_slice(&committed).unwrap();
        assert_eq!(
            read["result"].as_f64().unwrap().to_bits(),
            value.to_bits(),
            "JSON evidence changed: {}",
            String::from_utf8_lossy(&committed)
        );
        assert_eq!(serde_json::to_vec(&read).unwrap(), committed);
    }
}
