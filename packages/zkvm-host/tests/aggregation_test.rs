//! Edge cases for the `aggregation` circuit's host reference implementation.

use veria_zkvm_host::circuits::ref_aggregation;
use veria_zkvm_host::inputs::AggregationInput;
use veria_zkvm_host::prover::{ProveOptions, SpProver};

fn opts() -> ProveOptions {
    ProveOptions {
        real_proof: false,
        cross_check: true,
    }
}

#[test]
fn random_pile_matches_naive() {
    let data: Vec<u64> = vec![17, 42, 99, 3, 256, 1, 7, 1024, 33, 500];
    let out = ref_aggregation(&AggregationInput { data: data.clone() }).unwrap();
    let sum: u128 = data.iter().map(|&v| v as u128).sum();
    let avg: u64 = (sum / data.len() as u128) as u64;
    let lo = *data.iter().min().unwrap();
    let hi = *data.iter().max().unwrap();
    assert_eq!(out.sum_u128, sum);
    assert_eq!(out.avg_u64, avg);
    assert_eq!(out.min_u64, lo);
    assert_eq!(out.max_u64, hi);
    assert_eq!(out.count, data.len() as u32);
}

#[test]
fn monotone_sequence() {
    let data: Vec<u64> = (1u64..=100).collect();
    let out = ref_aggregation(&AggregationInput { data: data.clone() }).unwrap();
    assert_eq!(out.sum_u128, (1 + 100) * 100 / 2);
    assert_eq!(out.avg_u64, 50);
    assert_eq!(out.min_u64, 1);
    assert_eq!(out.max_u64, 100);
}

#[test]
fn single_element() {
    let data = vec![7u64];
    let out = ref_aggregation(&AggregationInput { data }).unwrap();
    assert_eq!(out.sum_u128, 7);
    assert_eq!(out.avg_u64, 7);
    assert_eq!(out.min_u64, 7);
    assert_eq!(out.max_u64, 7);
}

#[test]
fn empty_input() {
    let data: Vec<u64> = vec![];
    let out = ref_aggregation(&AggregationInput { data }).unwrap();
    assert_eq!(out.sum_u128, 0);
    assert_eq!(out.avg_u64, 0);
    assert_eq!(out.min_u64, 0);
    assert_eq!(out.max_u64, 0);
    assert_eq!(out.count, 0);
}

#[test]
fn out_of_bounds_rejected() {
    let data = vec![1u64; veria_zkvm_host::inputs::AGG_MAX + 1];
    let res = ref_aggregation(&AggregationInput { data });
    assert!(res.is_err());
}

#[test]
fn prover_simulator_path() {
    let inp = AggregationInput {
        data: vec![100, 200, 300],
    };
    let prover = SpProver::new();
    let out = prover.run_aggregation(&inp, &opts()).unwrap();
    assert!(!out.real);
    assert_eq!(out.cycles, 0);
}
