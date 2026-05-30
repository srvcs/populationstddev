use axum::body::Body;
use axum::extract::Json as JsonExtract;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router as AxumRouter};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use srvcs_populationstddev::{api::Deps, health, router, telemetry};
use tower::ServiceExt;

const DEAD_URL: &str = "http://127.0.0.1:1";

async fn serve(app: AxumRouter) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Mock `srvcs-populationvariance` that ACTUALLY COMPUTES the population
/// variance of the `values` array: the mean of the squared deviations from the
/// arithmetic mean. Returns `{"values", "result": <f64>}`.
async fn spawn_computing_populationvariance() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let nums: Vec<f64> = req["values"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default();
            let n = nums.len() as f64;
            let mean = nums.iter().sum::<f64>() / n;
            let variance = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
            Json(json!({ "values": req["values"], "result": variance }))
        }),
    );
    serve(app).await
}

/// Mock `srvcs-sqrt` that ACTUALLY COMPUTES `value.sqrt()` as an `f64`.
async fn spawn_computing_sqrt() -> String {
    let app = AxumRouter::new().route(
        "/",
        post(|JsonExtract(req): JsonExtract<Value>| async move {
            let value = req["value"].as_f64().unwrap_or(0.0);
            Json(json!({ "value": value, "result": value.sqrt() }))
        }),
    );
    serve(app).await
}

/// Mock that always answers with a fixed status + body (used to simulate a
/// `422` rejection forwarded from a dependency).
async fn spawn_fixed(status: StatusCode, body: Value) -> String {
    let app = AxumRouter::new().route(
        "/",
        post(move || {
            let body = body.clone();
            async move { (status, Json(body)) }
        }),
    );
    serve(app).await
}

fn app(populationvariance_url: &str, sqrt_url: &str) -> axum::Router {
    router(
        telemetry::metrics_handle_for_tests(),
        Deps {
            populationvariance_url: populationvariance_url.to_string(),
            sqrt_url: sqrt_url.to_string(),
        },
    )
}

async fn eval(populationvariance_url: &str, sqrt_url: &str, values: Value) -> (StatusCode, Value) {
    let res = app(populationvariance_url, sqrt_url)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "values": values }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn status_of(uri: &str) -> StatusCode {
    app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

fn approx(got: &Value, expected: f64) -> bool {
    got.as_f64().map(|x| (x - expected).abs() < 1e-9) == Some(true)
}

// --- Standard endpoints ---

#[tokio::test]
async fn healthz_ok() {
    assert_eq!(status_of("/healthz").await, StatusCode::OK);
}

#[tokio::test]
async fn readyz_reflects_state() {
    health::set_ready(true);
    assert_eq!(status_of("/readyz").await, StatusCode::OK);
}

#[tokio::test]
async fn openapi_ok() {
    assert_eq!(status_of("/openapi.json").await, StatusCode::OK);
}

#[tokio::test]
async fn index_reports_identity() {
    let res = app(DEAD_URL, DEAD_URL)
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["service"], "srvcs-populationstddev");
    assert_eq!(body["concern"], "statistics: population standard deviation");
    assert_eq!(
        body["depends_on"],
        json!(["srvcs-populationvariance", "srvcs-sqrt"])
    );
}

// --- Correctness cases, exercised against REAL computing dependencies ---

#[tokio::test]
async fn stddev_of_one_to_five() {
    let var = spawn_computing_populationvariance().await;
    let sqrt = spawn_computing_sqrt().await;
    // variance([1,2,3,4,5]) = 2.0; sqrt(2) ~= 1.4142135623730951
    let (status, body) = eval(&var, &sqrt, json!([1, 2, 3, 4, 5])).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        approx(&body["result"], std::f64::consts::SQRT_2),
        "got {:?}",
        body["result"]
    );
    assert_eq!(body["values"], json!([1, 2, 3, 4, 5]));
}

#[tokio::test]
async fn stddev_of_constant_list_is_zero() {
    let var = spawn_computing_populationvariance().await;
    let sqrt = spawn_computing_sqrt().await;
    // variance([7,7,7]) = 0; sqrt(0) = 0
    let (status, body) = eval(&var, &sqrt, json!([7, 7, 7])).await;
    assert_eq!(status, StatusCode::OK);
    assert!(approx(&body["result"], 0.0), "got {:?}", body["result"]);
}

#[tokio::test]
async fn stddev_of_two_values() {
    let var = spawn_computing_populationvariance().await;
    let sqrt = spawn_computing_sqrt().await;
    // mean([2,4]) = 3; variance = ((1)^2 + (1)^2)/2 = 1; sqrt(1) = 1
    let (status, body) = eval(&var, &sqrt, json!([2, 4])).await;
    assert_eq!(status, StatusCode::OK);
    assert!(approx(&body["result"], 1.0), "got {:?}", body["result"]);
}

#[tokio::test]
async fn stddev_with_negatives_and_floats() {
    let var = spawn_computing_populationvariance().await;
    let sqrt = spawn_computing_sqrt().await;
    // [-2.0, 0.0, 2.0]: mean 0; variance = (4 + 0 + 4)/3 = 8/3; sqrt(8/3) ~= 1.632993161855452
    let (status, body) = eval(&var, &sqrt, json!([-2.0, 0.0, 2.0])).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        approx(&body["result"], (8.0_f64 / 3.0).sqrt()),
        "got {:?}",
        body["result"]
    );
}

// --- Error / edge cases ---

#[tokio::test]
async fn forwards_422_from_populationvariance() {
    let var = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "values must be a non-empty list" }),
    )
    .await;
    let sqrt = spawn_computing_sqrt().await;
    let (status, body) = eval(&var, &sqrt, json!([])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "values must be a non-empty list");
}

#[tokio::test]
async fn forwards_422_for_non_numeric_element() {
    let var = spawn_fixed(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "error": "value is not a number" }),
    )
    .await;
    let sqrt = spawn_computing_sqrt().await;
    let (status, body) = eval(&var, &sqrt, json!([1, "nope", 3])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "value is not a number");
}

#[tokio::test]
async fn degrades_when_populationvariance_unreachable() {
    let sqrt = spawn_computing_sqrt().await;
    let (status, body) = eval(DEAD_URL, &sqrt, json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-populationvariance");
}

#[tokio::test]
async fn degrades_when_sqrt_unreachable() {
    let var = spawn_computing_populationvariance().await;
    let (status, body) = eval(&var, DEAD_URL, json!([1, 2, 3])).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["dependency"], "srvcs-sqrt");
}
