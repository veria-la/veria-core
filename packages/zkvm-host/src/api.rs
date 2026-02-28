//! HTTP bridge for the SP1 host.
//!
//! The FastAPI Compute API in the private Compute API service calls into this Axum service to
//! enqueue proof jobs. We expose three routes:
//!
//! * `GET  /healthz` — liveness.
//! * `GET  /circuits` — list of registered circuit ids.
//! * `POST /fold` — body `{ circuit, items: [json, ...] }`. Runs each item
//!   through [`crate::prover::SpProver`], folds the outputs with
//!   [`crate::folding::FoldingAdapter`], and returns the [`FoldedProof`]
//!   alongside per-sub-proof metadata.
//!
//! The bridge is intentionally narrow — proof bytes, cycle counts, and
//! accumulator state.  Anything fancier (job queue, persistence) lives in the
//! FastAPI layer.

use crate::circuits::CircuitId;
use crate::error::{HostError, HostResult};
use crate::folding::{FoldedProof, FoldingAdapter};
use crate::prover::{ProveOptions, ProveOutput, SpProver};
use crate::VERIA_HOST_VERSION;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Shared application state.
pub struct AppState {
    pub prover: SpProver,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            prover: SpProver::new(),
        }
    }
}

/// Build the Axum router.
pub fn router() -> Router {
    let state = Arc::new(AppState::default());
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    Router::new()
        .route("/healthz", get(healthz))
        .route("/circuits", get(list_circuits))
        .route("/fold", post(fold_handler))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERIA_HOST_VERSION,
    })
}

#[derive(Serialize)]
struct CircuitsResponse {
    circuits: Vec<CircuitMeta>,
}

#[derive(Serialize)]
struct CircuitMeta {
    id: u8,
    name: &'static str,
    elf_embedded: bool,
}

async fn list_circuits() -> Json<CircuitsResponse> {
    Json(CircuitsResponse {
        circuits: CircuitId::ALL
            .iter()
            .map(|c| CircuitMeta {
                id: *c as u8,
                name: c.name(),
                elf_embedded: c.elf_embedded(),
            })
            .collect(),
    })
}

#[derive(Deserialize)]
pub struct FoldRequest {
    pub circuit: String,
    pub items: Vec<serde_json::Value>,
    #[serde(default)]
    pub real_proof: bool,
}

#[derive(Serialize)]
pub struct FoldResponse {
    pub folded: FoldedProof,
    pub sub_proofs: Vec<SubProofSummary>,
    pub real: bool,
}

#[derive(Serialize)]
pub struct SubProofSummary {
    pub circuit: CircuitId,
    pub public_hash_hex: String,
    pub cycles: u64,
}

async fn fold_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FoldRequest>,
) -> Result<Json<FoldResponse>, ErrorEnvelope> {
    let circuit = CircuitId::from_str(&req.circuit)
        .map_err(ErrorEnvelope::from)?;
    if req.items.is_empty() {
        return Err(ErrorEnvelope::from(HostError::Folding(
            "fold requires at least one item".to_string(),
        )));
    }
    let mut outs: Vec<ProveOutput> = Vec::with_capacity(req.items.len());
    let opts = ProveOptions {
        real_proof: req.real_proof,
        cross_check: true,
    };
    for item in &req.items {
        let bytes = serde_json::to_vec(item).map_err(|e| {
            ErrorEnvelope::from(HostError::InvalidInput {
                bytes: 0,
                source: e,
            })
        })?;
        let out = state
            .prover
            .run_json(circuit, &bytes, &opts)
            .map_err(ErrorEnvelope::from)?;
        outs.push(out);
    }
    let folded = FoldingAdapter::fold_all(&outs).map_err(ErrorEnvelope::from)?;
    let real = outs.iter().any(|o| o.real);
    let sub_proofs = outs
        .into_iter()
        .map(|o| SubProofSummary {
            circuit: o.circuit,
            public_hash_hex: hex::encode(o.public_hash),
            cycles: o.cycles,
        })
        .collect();
    Ok(Json(FoldResponse {
        folded,
        sub_proofs,
        real,
    }))
}

/// JSON-shaped error envelope returned to HTTP callers.
#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
    #[serde(skip_serializing)]
    status: StatusCode,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl From<HostError> for ErrorEnvelope {
    fn from(e: HostError) -> Self {
        let status = match &e {
            HostError::UnknownCircuit(_) | HostError::InvalidInput { .. } => {
                StatusCode::BAD_REQUEST
            }
            HostError::OutOfBounds { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            error: ErrorBody {
                code: e.code(),
                message: e.to_string(),
            },
            status,
        }
    }
}

impl IntoResponse for ErrorEnvelope {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(serde_json::json!({"error": self.error}))).into_response()
    }
}

/// Bind the router to a TCP listener.  Used by the CLI `serve` subcommand.
pub async fn serve(addr: std::net::SocketAddr) -> HostResult<()> {
    let app = router();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(HostError::Io)?;
    tracing::info!(?addr, "veria-host HTTP bridge listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| HostError::Http(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn router_builds() {
        let _ = router();
    }

    #[test]
    fn unknown_circuit_maps_to_400() {
        let env = ErrorEnvelope::from(HostError::UnknownCircuit("nope".into()));
        assert_eq!(env.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn out_of_bounds_maps_to_413() {
        let env = ErrorEnvelope::from(HostError::OutOfBounds { got: 9, max: 8 });
        assert_eq!(env.status, StatusCode::PAYLOAD_TOO_LARGE);
    }
}
