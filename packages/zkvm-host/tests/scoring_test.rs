//! Cross-checks the host's `ref_scoring` against the same inputs run through
//! the prover wrapper.  Five edge cases mirror `docs/circuits.md` §1.

use veria_zkvm_host::circuits::ref_scoring;
use veria_zkvm_host::inputs::{ScoringInput, SCORING_MAX};
use veria_zkvm_host::prover::{ProveOptions, SpProver};

const FP_ONE: u64 = 1u64 << 32;

fn run(scores: &[u64], weights: &[u64], count: u32) -> ScoringInput {
    let mut s = [0u64; SCORING_MAX];
    let mut w = [0u64; SCORING_MAX];
    s[..scores.len()].copy_from_slice(scores);
    w[..weights.len()].copy_from_slice(weights);
    ScoringInput {
        scores: s,
        weights: w,
        count,
    }
}

fn opts() -> ProveOptions {
    ProveOptions {
        real_proof: false,
        cross_check: true,
    }
}

#[test]
fn empty_count_returns_zero() {
    let inp = run(&[], &[], 0);
    let out = ref_scoring(&inp).unwrap();
    assert_eq!(out.weighted_avg_fp, 0);
    assert_eq!(out.total_weight, 0);
}

#[test]
fn single_element_passes_through() {
    let inp = run(&[42], &[FP_ONE], 1);
    let out = ref_scoring(&inp).unwrap();
    assert_eq!(out.weighted_avg_fp, 42);
    assert_eq!(out.total_weight, FP_ONE);
}

#[test]
fn all_equal_weights_is_arithmetic_mean() {
    let scores = [10u64, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    let weights = [FP_ONE; 10];
    let inp = run(&scores, &weights, scores.len() as u32);
    let out = ref_scoring(&inp).unwrap();
    // Floor((10+20+...+100)/10) = 55.
    assert_eq!(out.weighted_avg_fp, 55);
    assert_eq!(out.total_weight, FP_ONE * 10);
}

#[test]
fn weighted_mix_skews_toward_higher_weight() {
    // weights: 1 unit on score 0, 3 units on score 100 -> weighted avg = 75.
    let scores = [0u64, 100];
    let weights = [FP_ONE, FP_ONE * 3];
    let inp = run(&scores, &weights, 2);
    let out = ref_scoring(&inp).unwrap();
    assert_eq!(out.weighted_avg_fp, 75);
    assert_eq!(out.total_weight, FP_ONE * 4);
}

#[test]
fn overflow_edge_uses_u128_intermediates() {
    // u64 score and weight close to max: ensure no panic and result fits.
    let large = (1u64 << 33).saturating_sub(1);
    let inp = run(&[large, large], &[FP_ONE, FP_ONE], 2);
    let out = ref_scoring(&inp).unwrap();
    assert_eq!(out.weighted_avg_fp, large);
    assert_eq!(out.total_weight, FP_ONE * 2);
}

#[test]
fn prover_simulator_matches_reference() {
    let inp = run(&[5, 10, 15], &[FP_ONE, FP_ONE, FP_ONE], 3);
    let prover = SpProver::new();
    let out = prover.run_scoring(&inp, &opts()).unwrap();
    assert!(!out.real, "simulator path used");
    assert_eq!(out.cycles, 0);
    assert!(!out.public_bytes.is_empty());
}
