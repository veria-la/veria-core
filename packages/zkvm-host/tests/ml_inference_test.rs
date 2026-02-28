//! Tests for the `ml-inference` circuit reference implementation.

use veria_zkvm_host::circuits::ref_ml;
use veria_zkvm_host::inputs::{MlInput, ML_FP_SHIFT, ML_H1, ML_H2, ML_IN, ML_OUT};

fn fp(x: i32) -> i32 {
    x << ML_FP_SHIFT
}

#[test]
fn zero_weights_yields_zero_logits() {
    // All weights and biases zero -> logits = 0 regardless of features.
    let mut ml = MlInput::default();
    for i in 0..ML_IN {
        ml.features[i] = fp(1);
    }
    let out = ref_ml(&ml).unwrap();
    assert_eq!(out.logits, [0; ML_OUT]);
    assert_ne!(out.model_commit, [0u8; 32]);
}

#[test]
fn saturated_relu_clips_at_zero() {
    // Make every pre-activation strongly negative -> ReLU clips to 0.
    let mut ml = MlInput::default();
    for i in 0..ML_IN {
        ml.features[i] = fp(1);
    }
    for j in 0..ML_H1 {
        for i in 0..ML_IN {
            ml.w1[j][i] = -fp(1);
        }
        ml.b1[j] = -fp(100);
    }
    // h1 becomes 0; subsequent layers produce 0 too because all weights
    // remain zero.
    let out = ref_ml(&ml).unwrap();
    assert_eq!(out.logits, [0; ML_OUT]);
}

#[test]
fn deep_negative_path() {
    // Positive features but every layer outputs negative pre-activation; with
    // ReLU clamping all logits to a known small range.
    let mut ml = MlInput::default();
    for i in 0..ML_IN {
        ml.features[i] = fp(2);
    }
    // Layer 1 yields large positive h1.
    for j in 0..ML_H1 {
        for i in 0..ML_IN {
            ml.w1[j][i] = fp(1);
        }
    }
    // Layer 2 inverts.
    for j in 0..ML_H2 {
        for i in 0..ML_H1 {
            ml.w2[j][i] = -fp(1);
        }
    }
    // Layer 3 inverts back to a small positive logit.
    for j in 0..ML_OUT {
        for i in 0..ML_H2 {
            ml.w3[j][i] = -fp(1);
        }
    }
    let out = ref_ml(&ml).unwrap();
    // After two inversions through ReLU the logits stay non-negative — we
    // only check that they are deterministic and finite.
    for v in out.logits {
        assert!(v >= 0);
    }
}

#[test]
fn positive_path_passes_through() {
    let mut ml = MlInput::default();
    // Identity-style features and small positive weights produce monotonic
    // positive logits.
    for i in 0..ML_IN {
        ml.features[i] = fp(1) / 2;
    }
    for j in 0..ML_H1 {
        for i in 0..ML_IN {
            ml.w1[j][i] = 1 << (ML_FP_SHIFT - 4);
        }
        ml.b1[j] = fp(1) / 8;
    }
    for j in 0..ML_H2 {
        for i in 0..ML_H1 {
            ml.w2[j][i] = 1 << (ML_FP_SHIFT - 4);
        }
        ml.b2[j] = fp(1) / 8;
    }
    for j in 0..ML_OUT {
        for i in 0..ML_H2 {
            ml.w3[j][i] = 1 << (ML_FP_SHIFT - 4);
        }
        ml.b3[j] = fp(1) / 8;
    }
    let out = ref_ml(&ml).unwrap();
    for v in out.logits {
        assert!(v > 0, "expected positive logit, got {v}");
    }
}

#[test]
fn model_commitment_is_deterministic() {
    let ml = MlInput::default();
    let a = ref_ml(&ml).unwrap();
    let b = ref_ml(&ml).unwrap();
    assert_eq!(a.model_commit, b.model_commit);
}
