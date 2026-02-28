//! End-to-end test that all five circuits run through the host simulator,
//! produce stable public hashes, and fold into a single accumulator.

use veria_zkvm_host::circuits::CircuitId;
use veria_zkvm_host::folding::FoldingAdapter;
use veria_zkvm_host::inputs::{
    AggregationInput, MedianInput, MlInput, ScoringInput, SortInput, MEDIAN_MAX, ML_FP_SHIFT,
    ML_H1, ML_H2, ML_IN, ML_OUT, SORT_MAX,
};
use veria_zkvm_host::prover::{ProveOptions, SpProver};

fn opts() -> ProveOptions {
    ProveOptions {
        real_proof: false,
        cross_check: true,
    }
}

fn scoring_input() -> ScoringInput {
    let scores = [10u64, 20, 30, 40];
    let weights = [
        1u64 << ML_FP_SHIFT,
        1u64 << ML_FP_SHIFT,
        1u64 << ML_FP_SHIFT,
        1u64 << ML_FP_SHIFT,
    ];
    ScoringInput::new(&scores, &weights).expect("scoring fits")
}

fn agg_input() -> AggregationInput {
    AggregationInput {
        data: (1u64..=100).collect(),
    }
}

fn median_input() -> MedianInput {
    let raw: [u64; MEDIAN_MAX] = {
        let mut a = [0u64; MEDIAN_MAX];
        for i in 0..7 {
            a[i] = (7 - i) as u64;
        }
        a
    };
    let mut sorted = raw;
    let n = 7;
    sorted[..n].sort();
    // perm[i] = original index in raw of sorted[i]
    let mut perm = [0u16; MEDIAN_MAX];
    let mut taken = [false; MEDIAN_MAX];
    for i in 0..n {
        for j in 0..n {
            if !taken[j] && raw[j] == sorted[i] {
                perm[i] = j as u16;
                taken[j] = true;
                break;
            }
        }
    }
    MedianInput {
        raw,
        sorted,
        perm,
        count: n as u32,
    }
}

fn sort_input() -> SortInput {
    let mut input = [0u64; SORT_MAX];
    let raw_seq = [9u64, 3, 7, 1, 5, 8, 2, 4, 6];
    for (i, v) in raw_seq.iter().enumerate() {
        input[i] = *v;
    }
    let n = raw_seq.len();
    let mut sorted = input;
    sorted[..n].sort();
    let mut perm = [0u16; SORT_MAX];
    let mut taken = [false; SORT_MAX];
    for i in 0..n {
        for j in 0..n {
            if !taken[j] && input[j] == sorted[i] {
                perm[i] = j as u16;
                taken[j] = true;
                break;
            }
        }
    }
    SortInput {
        input,
        sorted,
        perm,
        count: n as u32,
    }
}

fn ml_input() -> MlInput {
    let mut ml = MlInput::default();
    // Identity-ish features.
    for i in 0..ML_IN {
        ml.features[i] = (1i32 << ML_FP_SHIFT) / 2;
    }
    // Small positive weights so we get a positive logit on every output.
    for j in 0..ML_H1 {
        for i in 0..ML_IN {
            ml.w1[j][i] = 1 << (ML_FP_SHIFT - 4);
        }
        ml.b1[j] = 0;
    }
    for j in 0..ML_H2 {
        for i in 0..ML_H1 {
            ml.w2[j][i] = 1 << (ML_FP_SHIFT - 4);
        }
        ml.b2[j] = 0;
    }
    for j in 0..ML_OUT {
        for i in 0..ML_H2 {
            ml.w3[j][i] = 1 << (ML_FP_SHIFT - 4);
        }
        ml.b3[j] = 0;
    }
    ml
}

#[test]
fn all_five_circuits_execute_and_fold() {
    let p = SpProver::new();
    let outs = vec![
        p.run_scoring(&scoring_input(), &opts()).unwrap(),
        p.run_aggregation(&agg_input(), &opts()).unwrap(),
        p.run_median(&median_input(), &opts()).unwrap(),
        p.run_sort(&sort_input(), &opts()).unwrap(),
        p.run_ml(&ml_input(), &opts()).unwrap(),
    ];
    // Determinism: re-running yields the same public hashes.
    let outs2 = vec![
        p.run_scoring(&scoring_input(), &opts()).unwrap(),
        p.run_aggregation(&agg_input(), &opts()).unwrap(),
        p.run_median(&median_input(), &opts()).unwrap(),
        p.run_sort(&sort_input(), &opts()).unwrap(),
        p.run_ml(&ml_input(), &opts()).unwrap(),
    ];
    for (a, b) in outs.iter().zip(outs2.iter()) {
        assert_eq!(a.public_hash, b.public_hash, "non-deterministic circuit");
    }

    let mut seen_circuits = Vec::new();
    for o in &outs {
        seen_circuits.push(o.circuit);
    }
    assert_eq!(
        seen_circuits,
        vec![
            CircuitId::Scoring,
            CircuitId::Aggregation,
            CircuitId::Median,
            CircuitId::Sort,
            CircuitId::MlInference,
        ]
    );

    // Fold the mixed batch — must take SuperNova (heterogeneous) path.
    let folded = FoldingAdapter::fold_all(&outs).unwrap();
    assert_eq!(folded.n, 5);
    assert!(!folded.homogeneous);
    folded.check().expect("accumulator stable");
}
