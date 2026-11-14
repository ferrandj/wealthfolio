#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
//! The app's answers travel compressed when the browser takes it, and read to their end.
use axum::{body::Body, http::Request};
use tempfile::tempdir;
use tower::ServiceExt;
use wealthfolio_server::{
    api::{app_router_from_config, compression},
    config::Config,
};

#[tokio::test]
async fn an_answer_is_compressed_for_a_browser_that_takes_it() {
    let tmp = tempdir().unwrap();
    std::env::set_var("WF_DATA_DIR", tmp.path());
    std::env::set_var("WF_DB_PATH", tmp.path().join("test.db"));
    std::env::set_var("WF_SECRET_KEY", "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    std::env::set_var("WF_LISTEN_ADDR", "127.0.0.1:0");
    std::env::set_var("WF_AUTH_REQUIRED", "false");
    let config = Config::from_env().unwrap();
    let app = app_router_from_config(&config)
        .await
        .unwrap()
        .layer(compression());

    let ask = |encoding: Option<&str>| {
        let mut request = Request::builder().uri("/api/v1/statements/supported");
        if let Some(encoding) = encoding {
            request = request.header("accept-encoding", encoding);
        }
        app.clone().oneshot(request.body(Body::empty()).unwrap())
    };
    let plain = ask(None).await.unwrap();
    assert_eq!(plain.status(), 200);
    assert!(plain.headers().get("content-encoding").is_none());
    let plain = axum::body::to_bytes(plain.into_body(), usize::MAX)
        .await
        .unwrap();

    let packed = ask(Some("gzip")).await.unwrap();
    assert_eq!(packed.status(), 200);
    assert_eq!(packed.headers()["content-encoding"], "gzip");
    // read to the end: the body is asked once more after its last piece
    let packed = axum::body::to_bytes(packed.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(packed.len() * 2 < plain.len());

    for key in [
        "WF_DATA_DIR",
        "WF_DB_PATH",
        "WF_SECRET_KEY",
        "WF_LISTEN_ADDR",
        "WF_AUTH_REQUIRED",
    ] {
        std::env::remove_var(key);
    }
}
