use std::env;

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
use sqlx::{FromRow, PgPool};

#[derive(Serialize, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct UserInput {
    name: String,
    email: String,
}

struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        eprintln!("database error: {err}");
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    }
}

const NOT_FOUND: ApiError = ApiError(StatusCode::NOT_FOUND, "user not found");
const INVALID_ID: ApiError = ApiError(StatusCode::BAD_REQUEST, "invalid id");
const INVALID_INPUT: ApiError = ApiError(StatusCode::BAD_REQUEST, "name and email are required");

fn user_id(path: Result<Path<i64>, PathRejection>) -> Result<i64, ApiError> {
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

async fn list_users(State(pool): State<PgPool>) -> Result<Json<Vec<User>>, ApiError> {
    let users = sqlx::query_as("SELECT id, name, email FROM users ORDER BY id")
        .fetch_all(&pool)
        .await?;
    Ok(Json(users))
}

async fn get_user(
    State(pool): State<PgPool>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<User>, ApiError> {
    let id = user_id(path)?;
    sqlx::query_as("SELECT id, name, email FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .map(Json)
        .ok_or(NOT_FOUND)
}

async fn create_user(
    State(pool): State<PgPool>,
    body: Result<Json<UserInput>, JsonRejection>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let input = user_input(body)?;
    let user = sqlx::query_as(
        "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email",
    )
    .bind(input.name)
    .bind(input.email)
    .fetch_one(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(
    State(pool): State<PgPool>,
    path: Result<Path<i64>, PathRejection>,
    body: Result<Json<UserInput>, JsonRejection>,
) -> Result<Json<User>, ApiError> {
    let id = user_id(path)?;
    let input = user_input(body)?;
    sqlx::query_as(
        "UPDATE users SET name = $1, email = $2 WHERE id = $3 RETURNING id, name, email",
    )
    .bind(input.name)
    .bind(input.email)
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .map(Json)
    .ok_or(NOT_FOUND)
}

async fn delete_user(
    State(pool): State<PgPool>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let id = user_id(path)?;
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    match result.rows_affected() {
        0 => Err(NOT_FOUND),
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

#[tokio::main]
async fn main() {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://api:api@localhost:5432/users".to_string());
    let pool = PgPool::connect(&database_url)
        .await
        .expect("failed to connect to Postgres");

    let app = Router::new()
        .route("/users", get(list_users).post(create_user))
        .route(
            "/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .with_state(pool);

    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
