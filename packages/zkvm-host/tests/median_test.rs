//! Tests for the `median` circuit reference implementation.

use veria_zkvm_host::circuits::ref_median;
use veria_zkvm_host::inputs::{MedianInput, MEDIAN_MAX};

fn build(raw_seq: &[u64]) -> MedianInput {
    let n = raw_seq.len();
    let mut raw = [0u64; MEDIAN_MAX];
    raw[..n].copy_from_slice(raw_seq);
    let mut sorted = raw;
    sorted[..n].sort();
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

#[test]
fn odd_count() {
    // [1,2,3,4,5] median = 3
    let inp = build(&[5, 3, 1, 4, 2]);
    let out = ref_median(&inp).unwrap();
    assert_eq!(out.median, 3);
    assert_eq!(out.count, 5);
}

#[test]
fn even_count_takes_lower_middle() {
    // [10,20,30,40] -> lower middle = 20
    let inp = build(&[40, 10, 30, 20]);
    let out = ref_median(&inp).unwrap();
    assert_eq!(out.median, 20);
    assert_eq!(out.count, 4);
}

#[test]
fn all_duplicates() {
    let inp = build(&[7; 9]);
    let out = ref_median(&inp).unwrap();
    assert_eq!(out.median, 7);
}

#[test]
fn already_monotone() {
    let inp = build(&[1, 2, 3, 4, 5, 6, 7]);
    let out = ref_median(&inp).unwrap();
    assert_eq!(out.median, 4);
}

#[test]
fn reverse_input() {
    let inp = build(&[9, 8, 7, 6, 5, 4, 3, 2, 1]);
    let out = ref_median(&inp).unwrap();
    assert_eq!(out.median, 5);
}

#[test]
fn bad_perm_rejected() {
    // Tamper with perm.
    let mut inp = build(&[3, 1, 2]);
    inp.perm[0] = 7; // out of range for count=3
    assert!(ref_median(&inp).is_err());
}

#[test]
fn bad_sorted_rejected() {
    let mut inp = build(&[1, 2, 3]);
    inp.sorted[1] = 999;
    assert!(ref_median(&inp).is_err());
}
