use std::{
    collections::BTreeMap,
    env,
    sync::{Arc, RwLock},
};

use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct UserInput {
    name: String,
    email: String,
}

#[derive(Default)]
struct Store {
    users: BTreeMap<u64, User>,
    next_id: u64,
}

type AppState = Arc<RwLock<Store>>;

struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

const NOT_FOUND: ApiError = ApiError(StatusCode::NOT_FOUND, "user not found");
const INVALID_ID: ApiError = ApiError(StatusCode::BAD_REQUEST, "invalid id");
const INVALID_INPUT: ApiError = ApiError(StatusCode::BAD_REQUEST, "name and email are required");

fn user_id(path: Result<Path<u64>, PathRejection>) -> Result<u64, ApiError> {
    match path {
        Ok(Path(id)) if id > 0 => Ok(id),
        _ => Err(INVALID_ID),
    }
}

fn user_input(body: Result<Json<UserInput>, JsonRejection>) -> Result<UserInput, ApiError> {
    match body {
        Ok(Json(input)) if !input.name.is_empty() && !input.email.is_empty() => Ok(input),
        _ => Err(INVALID_INPUT),
    }
}

async fn list_users(State(state): State<AppState>) -> Json<Vec<User>> {
    Json(state.read().unwrap().users.values().cloned().collect())
}

async fn get_user(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<Json<User>, ApiError> {
    let id = user_id(path)?;
    let store = state.read().unwrap();
    store.users.get(&id).cloned().map(Json).ok_or(NOT_FOUND)
}

async fn create_user(
    State(state): State<AppState>,
    body: Result<Json<UserInput>, JsonRejection>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let input = user_input(body)?;
    let mut store = state.write().unwrap();
    store.next_id += 1;
    let user = User {
        id: store.next_id,
        name: input.name,
        email: input.email,
    };
    store.users.insert(user.id, user.clone());
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
    body: Result<Json<UserInput>, JsonRejection>,
) -> Result<Json<User>, ApiError> {
    let id = user_id(path)?;
    let input = user_input(body)?;
    let mut store = state.write().unwrap();
    let user = store.users.get_mut(&id).ok_or(NOT_FOUND)?;
    user.name = input.name;
    user.email = input.email;
    Ok(Json(user.clone()))
}

async fn delete_user(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let id = user_id(path)?;
    let mut store = state.write().unwrap();
    store
        .users
        .remove(&id)
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or(NOT_FOUND)
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/users", get(list_users).post(create_user))
        .route(
            "/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .with_state(AppState::default());

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
