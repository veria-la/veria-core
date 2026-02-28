//! Tests for the `sort` circuit reference implementation.

use veria_zkvm_host::circuits::ref_sort;
use veria_zkvm_host::inputs::{SortInput, SORT_MAX};

fn build(raw_seq: &[u64]) -> SortInput {
    let n = raw_seq.len();
    let mut input = [0u64; SORT_MAX];
    input[..n].copy_from_slice(raw_seq);
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

#[test]
fn random_input() {
    let inp = build(&[3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5]);
    let out = ref_sort(&inp).unwrap();
    assert_eq!(out.count, 11);
    assert_ne!(out.sorted_commit, [0u8; 32]);
    assert_ne!(out.input_commit, out.sorted_commit);
}

#[test]
fn with_duplicates() {
    let inp = build(&[7, 7, 7, 1, 1, 9, 9, 9]);
    let out = ref_sort(&inp).unwrap();
    assert_eq!(out.count, 8);
}

#[test]
fn presorted_input() {
    let inp = build(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    let out = ref_sort(&inp).unwrap();
    // When presorted, input_commit == sorted_commit by construction.
    assert_eq!(out.input_commit, out.sorted_commit);
}

#[test]
fn reverse_input() {
    let inp = build(&[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    let out = ref_sort(&inp).unwrap();
    assert_ne!(out.input_commit, out.sorted_commit);
}

#[test]
fn bad_sorted_rejected() {
    let mut inp = build(&[5, 1, 3]);
    inp.sorted[0] = 999; // breaks both monotonicity and multiset equality
    assert!(ref_sort(&inp).is_err());
}

#[test]
fn tampered_multiset_rejected() {
    let mut inp = build(&[5, 1, 3]);
    // Replace sorted with a different monotonic vector — passes monotonicity
    // but fails multiset equality.
    inp.sorted[0] = 2;
    inp.sorted[1] = 4;
    inp.sorted[2] = 6;
    assert!(ref_sort(&inp).is_err());
}
