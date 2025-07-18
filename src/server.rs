use axum::{
    response::{IntoResponse, Response},
    middleware::{ Next, from_fn_with_state },
    extract::{ Request, State, Path },
    routing::get,
    Router,
    Json,
};
use tokio_util::io::ReaderStream;
use std::path::PathBuf;
use tokio::fs::File;
use http::{header, StatusCode, HeaderValue};
use crate::jbod;
use rand::{distr::Alphanumeric, Rng};
use log::*;

#[derive(Clone)]
struct AppState {
    token: String,
    src_paths: Vec<String>,
}

async fn serve_large_file(Path(filename): Path<String>, State(state): State<AppState>) -> Response {
    let try_find = jbod::find_file(&state.src_paths, &PathBuf::from(&filename));
    if try_find.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = try_find.unwrap();
    info!("Got request: {}", path.display());

    match File::open(&path).await {
        Ok(file) => {
            let stream = ReaderStream::new(file);
            let body = axum::body::Body::from_stream(stream);

            let metadata = tokio::fs::metadata(&path).await.ok();
            let len = metadata.map(|m| m.len());

            let mut response = Response::new(body);
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_str("application/octet-stream").unwrap());

            if let Some(len) = len {
                headers.insert(header::CONTENT_LENGTH, len.to_string().parse().unwrap());
            }

            response
        }
        Err(_) => {
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

async fn get_file_list(State(state): State<AppState>) -> Response {
    Json(jbod::list_files(&state.src_paths)).into_response()
}

async fn check_auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let auth_header = req.headers().get("Authorization");
    if auth_header.map(HeaderValue::as_bytes) == Some(state.token.as_bytes()) {
        next.run(req).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

async fn async_serve(state: AppState, port: u16) {
    let app = Router::new()
        .route("/download/{*filename}", get(serve_large_file))
        .route("/list", get(get_file_list))
        .layer(from_fn_with_state(state.clone(), check_auth))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", port))
        .await
        .unwrap();
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();

}

pub fn serve(src_paths: Vec<String>, port: u16) {
    let token: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    info!("Bearer token for this session: {}", token);

    let state = AppState {
        token: format!("Bearer {token}"),
        src_paths
    };

    let rt = tokio::runtime::Builder::new_multi_thread().enable_io().enable_time()
        .build()
        .unwrap();
    rt.block_on(async move { async_serve(state, port).await });
}
