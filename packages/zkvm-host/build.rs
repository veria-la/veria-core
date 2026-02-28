// build.rs — VERIA zkVM host
//
// When `VERIA_BUILD_ELFS=1` is set, shells out to `cargo prove build` for
// every guest crate.  We invoke the Succinct toolchain via
// `std::process::Command` instead of pulling `sp1-build` into our
// build-dependencies because `cargo` does not let us feature-gate
// build-dependencies, and `sp1-build` transitively pulls openssl-sys via
// the SP1 SDK on Linux — that is not acceptable for plain `cargo check`.
//
// To force an ELF rebuild:
//
//     VERIA_BUILD_ELFS=1 cargo build -p veria-zkvm-host
//
// To override the binary that does the work (e.g. for a custom toolchain):
//
//     VERIA_PROVE_BIN=/path/to/cargo-prove cargo build -p veria-zkvm-host
//
// References:
//   * SP1 build helper: <https://docs.succinct.xyz/writing-programs/setup.html>

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Relative paths to each circuit's guest program crate, anchored at this
/// crate's manifest directory.  Edit this list when a new circuit is added.
const CIRCUITS: &[&str] = &[
    "../circuits/scoring/program",
    "../circuits/aggregation/program",
    "../circuits/median/program",
    "../circuits/sort/program",
    "../circuits/ml-inference/program",
];

fn main() {
    println!("cargo:rerun-if-env-changed=VERIA_BUILD_ELFS");
    println!("cargo:rerun-if-env-changed=VERIA_PROVE_BIN");
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set"));
    for c in CIRCUITS {
        let guest = manifest_dir.join(c);
        println!("cargo:rerun-if-changed={}", guest.join("src/main.rs").display());
        println!("cargo:rerun-if-changed={}", guest.join("Cargo.toml").display());
    }

    if env::var("VERIA_BUILD_ELFS").ok().as_deref() != Some("1") {
        println!(
            "cargo:warning=VERIA_BUILD_ELFS != 1; skipping SP1 guest ELF \
             compilation. Set VERIA_BUILD_ELFS=1 (and have `cargo-prove` \
             on PATH) to invoke the SP1 toolchain."
        );
        return;
    }

    let prove_bin = env::var("VERIA_PROVE_BIN").unwrap_or_else(|_| "cargo-prove".to_string());
    for c in CIRCUITS {
        let guest = manifest_dir.join(c);
        let status = Command::new(&prove_bin)
            .args(["prove", "build"])
            .current_dir(&guest)
            .status()
            .unwrap_or_else(|e| {
                panic!(
                    "failed to spawn `{prove_bin} prove build` in {}: {e}",
                    guest.display()
                )
            });
        if !status.success() {
            panic!(
                "`{prove_bin} prove build` failed in {}: status {status}",
                guest.display()
            );
        }
    }
}
