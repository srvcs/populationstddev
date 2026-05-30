use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-populationstddev";
pub const CONCERN: &str = "statistics: population standard deviation";
pub const DEPENDS_ON: &[&str] = &["srvcs-populationvariance", "srvcs-sqrt"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub populationvariance_url: String,
    pub sqrt_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    /// The list of numbers whose population standard deviation to compute.
    /// Validation (e.g. an empty list, or a non-numeric element) is propagated
    /// from `srvcs-populationvariance`'s `422`.
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct PopulationStddevResponse {
    #[schema(value_type = Object)]
    pub values: Vec<Value>,
    /// The population standard deviation, as an `f64`.
    pub result: f64,
}

fn ok(values: Vec<Value>, result: f64) -> Response {
    (
        StatusCode::OK,
        Json(json!({ "values": values, "result": result })),
    )
        .into_response()
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

/// Forward a dependency's response verbatim (used to propagate `422` for
/// invalid input from a dependency).
fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// A reachable dependency answered `200` but its body lacked a numeric
/// `result`. That is a contract violation we cannot recover from, so surface a
/// `500` rather than guessing.
fn malformed(dependency: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            json!({ "error": "dependency returned a malformed result", "dependency": dependency }),
        ),
    )
        .into_response()
}

/// Call one dependency at `url` with `body`, mapping its outcome to either the
/// parsed response body (on `200`) or an early-return `Response` the caller
/// should surface verbatim:
///
/// - unreachable / non-`200`/`422` -> `503` degraded
/// - `422` -> forwarded `422` (the dependency rejected the input)
async fn ask(url: &str, body: &Value, dependency: &str) -> Result<Value, Response> {
    match client::call(url, body).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => Ok(body),
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — the population standard deviation of a list of numbers, as an
/// `f64`.
///
/// This service owns the *control flow* but delegates every computation to its
/// dependencies, exactly as specified:
///
/// 1. `v = (call populationvariance {"values": values}).result`;
/// 2. `result = (call sqrt {"value": v}).result`.
///
/// This is a pure orchestrator: it does no validation of its own and never
/// calls `srvcs-isnumber` directly. An empty list or a non-numeric element is
/// rejected by `srvcs-populationvariance`, whose `422` is forwarded. If a
/// dependency is unreachable it reports itself degraded (`503`).
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = PopulationStddevResponse),
        (status = 422, description = "a dependency rejected the input (forwarded), e.g. an empty list or a non-numeric element"),
        (status = 500, description = "a dependency returned a malformed result"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    // 1. v = variance of the population, delegated wholesale.
    let variance_body = match ask(
        &deps.populationvariance_url,
        &json!({ "values": req.values }),
        "srvcs-populationvariance",
    )
    .await
    {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let v = match variance_body.get("result").and_then(Value::as_f64) {
        Some(v) => v,
        None => return malformed("srvcs-populationvariance"),
    };

    // 2. result = sqrt(v).
    let sqrt_body = match ask(&deps.sqrt_url, &json!({ "value": v }), "srvcs-sqrt").await {
        Ok(body) => body,
        Err(resp) => return resp,
    };
    let result = match sqrt_body.get("result").and_then(Value::as_f64) {
        Some(r) => r,
        None => return malformed("srvcs-sqrt"),
    };

    ok(req.values, result)
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, PopulationStddevResponse))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[tokio::test]
    async fn index_reports_all_dependencies() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-populationstddev");
        assert_eq!(info.concern, "statistics: population standard deviation");
        assert_eq!(
            info.depends_on,
            vec!["srvcs-populationvariance", "srvcs-sqrt"]
        );
    }
}
