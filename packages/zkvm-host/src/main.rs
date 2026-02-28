//! `veria-host` — CLI front-end for the SP1 zkVM host.
//!
//! Subcommands:
//!
//! * `circuits`          — list the registered circuits and their on-chain ids.
//! * `prove`             — run a single circuit on a JSON input and print the
//!                         resulting public values + hash.
//! * `fold`              — batch a JSON array of inputs through the same
//!                         circuit and return the folded accumulator.
//! * `serve`             — start the Axum HTTP bridge (default port 8088).
//!
//! All paths are deterministic and toolchain-free; the CLI does not require
//! `cargo prove` to be installed.

use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use veria_zkvm_host::api;
use veria_zkvm_host::circuits::CircuitId;
use veria_zkvm_host::error::HostError;
use veria_zkvm_host::folding::FoldingAdapter;
use veria_zkvm_host::prover::{ProveOptions, SpProver};
use veria_zkvm_host::VERIA_HOST_VERSION;

#[derive(Parser, Debug)]
#[command(
    name = "veria-host",
    version = VERIA_HOST_VERSION,
    about = "VERIA SP1 zkVM host — generate and fold proofs for the Solana mainnet verifier",
)]
struct Cli {
    /// Verbose tracing (`-v` info, `-vv` debug, `-vvv` trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List the five registered circuits.
    Circuits,
    /// Prove a single input.
    Prove {
        /// Circuit name (e.g. `scoring`, `ml-inference`).
        #[arg(long)]
        circuit: String,
        /// Path to the JSON input.
        #[arg(long)]
        input: PathBuf,
        /// Write the JSON proof output here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Request the real SP1 prover (slow, requires the cargo-prove
        /// toolchain).  When the ELF is missing this is silently ignored and
        /// the simulator is used.
        #[arg(long)]
        real: bool,
    },
    /// Fold a batch of inputs.
    Fold {
        /// Circuit name.
        #[arg(long)]
        circuit: String,
        /// JSON file whose top-level value is an array of inputs.
        #[arg(long)]
        inputs: PathBuf,
        /// Write the folded proof JSON here (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        real: bool,
    },
    /// Start the HTTP bridge.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8088")]
        addr: SocketAddr,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let res: Result<(), HostError> = match cli.cmd {
        Cmd::Circuits => cmd_circuits(),
        Cmd::Prove {
            circuit,
            input,
            out,
            real,
        } => cmd_prove(&circuit, &input, out.as_deref(), real),
        Cmd::Fold {
            circuit,
            inputs,
            out,
            real,
        } => cmd_fold(&circuit, &inputs, out.as_deref(), real),
        Cmd::Serve { addr } => cmd_serve(addr),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(error = %e, code = e.code(), "command failed");
            ExitCode::from(1)
        }
    }
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("veria_zkvm_host={level},veria_host={level}")));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn cmd_circuits() -> Result<(), HostError> {
    println!("VERIA circuits (host {VERIA_HOST_VERSION}, SP1 v3 target)");
    println!("{:>3}  {:<14}  {}", "id", "name", "elf");
    for c in CircuitId::ALL {
        let elf = if c.elf_embedded() { "embedded" } else { "simulator" };
        println!("{:>3}  {:<14}  {}", *c as u8, c.name(), elf);
    }
    Ok(())
}

fn cmd_prove(
    circuit: &str,
    input: &std::path::Path,
    out: Option<&std::path::Path>,
    real: bool,
) -> Result<(), HostError> {
    let c = CircuitId::from_str(circuit)?;
    let bytes = fs::read(input)?;
    let prover = SpProver::new();
    let opts = ProveOptions {
        real_proof: real,
        cross_check: true,
    };
    let result = prover.run_json(c, &bytes, &opts)?;
    info!(
        circuit = %c.name(),
        cycles = result.cycles,
        real = result.real,
        "proof complete",
    );
    let json = serde_json::to_vec_pretty(&result).map_err(|e| HostError::InvalidInput {
        bytes: 0,
        source: e,
    })?;
    write_or_stdout(out, &json)
}

fn cmd_fold(
    circuit: &str,
    inputs: &std::path::Path,
    out: Option<&std::path::Path>,
    real: bool,
) -> Result<(), HostError> {
    let c = CircuitId::from_str(circuit)?;
    let raw = fs::read(inputs)?;
    let parsed: Vec<serde_json::Value> =
        serde_json::from_slice(&raw).map_err(|e| HostError::InvalidInput {
            bytes: raw.len(),
            source: e,
        })?;
    if parsed.is_empty() {
        return Err(HostError::Folding(
            "input array must contain at least one element".to_string(),
        ));
    }
    let prover = SpProver::new();
    let opts = ProveOptions {
        real_proof: real,
        cross_check: true,
    };
    let mut adapter = FoldingAdapter::new();
    for (i, v) in parsed.iter().enumerate() {
        let bytes = serde_json::to_vec(v).map_err(|e| HostError::InvalidInput {
            bytes: 0,
            source: e,
        })?;
        let sub = prover.run_json(c, &bytes, &opts)?;
        info!(idx = i, "absorbed sub-proof");
        adapter.absorb(&sub)?;
    }
    let folded = adapter.finish()?;
    folded.check()?;
    let json = serde_json::to_vec_pretty(&folded).map_err(|e| HostError::InvalidInput {
        bytes: 0,
        source: e,
    })?;
    write_or_stdout(out, &json)
}

fn cmd_serve(addr: SocketAddr) -> Result<(), HostError> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(HostError::Io)?;
    rt.block_on(api::serve(addr))
}

fn write_or_stdout(out: Option<&std::path::Path>, bytes: &[u8]) -> Result<(), HostError> {
    use std::io::Write;
    match out {
        Some(p) => {
            fs::write(p, bytes)?;
        }
        None => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.write_all(b"\n")?;
        }
    }
    Ok(())
}
