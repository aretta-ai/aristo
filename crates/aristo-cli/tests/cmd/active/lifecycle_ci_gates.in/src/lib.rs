#[aristo::intent(
    "The CI gates scenario fixture: one neural-verified intent so the per-pipeline rate line has a non-zero numerator.",
    verify = "neural",
    id = "ci_gate_fixture_neural_verified"
)]
fn ci_neural_intent() {}

#[aristo::intent(
    "A second intent in `Status::Unknown` so the rate denominators don't all equal their numerators.",
    verify = "neural",
    id = "ci_gate_fixture_neural_unknown"
)]
fn ci_neural_unknown_intent() {}
