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
use chrono::{NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};

// Dates and timestamps are formatted by Postgres so all four APIs return identical strings.
// A macro rather than a const so concat! can build each query as a &'static str.
macro_rules! columns {
    () => {
        r#"id, name, email, to_char(birthday, 'YYYY-MM-DD') AS birthday, cpf,
        to_char(inserted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS inserted_at,
        to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at"#
    };
}

#[derive(Serialize, FromRow)]
struct User {
    id: i64,
    name: String,
    email: String,
    birthday: String,
    cpf: String,
    inserted_at: String,
    updated_at: String,
}

struct UserInput {
    name: String,
    email: String,
    birthday: NaiveDate,
    cpf: String,
}

struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        if let Some(db_err) = err.as_database_error().filter(|e| e.is_unique_violation()) {
            return match db_err.constraint() {
                Some("users_cpf_key") => CPF_TAKEN,
                _ => EMAIL_TAKEN,
            };
        }
        eprintln!("database error: {err}");
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    }
}

const NOT_FOUND: ApiError = ApiError(StatusCode::NOT_FOUND, "user not found");
const INVALID_ID: ApiError = ApiError(StatusCode::BAD_REQUEST, "invalid id");
const INVALID_BODY: ApiError = ApiError(StatusCode::BAD_REQUEST, "invalid request body");
const INVALID_NAME: ApiError = ApiError(StatusCode::BAD_REQUEST, "name must be a non-empty string");
const INVALID_EMAIL: ApiError = ApiError(
    StatusCode::BAD_REQUEST,
    "email must be a valid email address",
);
const INVALID_BIRTHDAY: ApiError = ApiError(
    StatusCode::BAD_REQUEST,
    "birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today",
);
const INVALID_CPF: ApiError = ApiError(StatusCode::BAD_REQUEST, "cpf must be a valid CPF");
const EMAIL_TAKEN: ApiError = ApiError(StatusCode::CONFLICT, "email already registered");
const CPF_TAKEN: ApiError = ApiError(StatusCode::CONFLICT, "cpf already registered");

fn user_id(path: Result<Path<i64>, PathRejection>) -> Result<i64, ApiError> {
    match path {
        Ok(Path(id)) if id > 0 => Ok(id),
        _ => Err(INVALID_ID),
    }
}

fn is_email(value: &str) -> bool {
    value.matches('@').count() == 1 && !value.starts_with('@') && !value.ends_with('@')
}

fn parse_birthday(value: &str) -> Option<NaiveDate> {
    // chrono alone would also accept unpadded values such as 1990-5-7.
    let is_iso = value.len() == 10
        && value.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .filter(|_| is_iso)?;
    let earliest = NaiveDate::from_ymd_opt(1900, 1, 1)?;
    (earliest <= date && date <= Utc::now().date_naive()).then_some(date)
}

/// Accepts 11 digits or the 000.000.000-00 mask, checks both verification digits.
fn normalize_cpf(value: &str) -> Option<String> {
    let plain = value.len() == 11 && value.bytes().all(|b| b.is_ascii_digit());
    let masked = value.len() == 14
        && value.bytes().enumerate().all(|(i, b)| match i {
            3 | 7 => b == b'.',
            11 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    if !plain && !masked {
        return None;
    }
    let digits: Vec<u32> = value.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.iter().all(|&d| d == digits[0]) {
        return None;
    }
    for n in [9, 10] {
        let total: u32 = (0..n).map(|i| digits[i] * (n + 1 - i) as u32).sum();
        if total * 10 % 11 % 10 != digits[n] {
            return None;
        }
    }
    Some(value.chars().filter(char::is_ascii_digit).collect())
}

/// Validates the body field by field, in the same order as the other APIs.
fn user_input(body: Result<Json<Value>, JsonRejection>) -> Result<UserInput, ApiError> {
    let Ok(Json(Value::Object(body))) = body else {
        return Err(INVALID_BODY);
    };
    let field = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
    let name = field("name")
        .filter(|v| !v.is_empty())
        .ok_or(INVALID_NAME)?;
    let email = field("email")
        .filter(|v| is_email(v))
        .ok_or(INVALID_EMAIL)?;
    let birthday = field("birthday")
        .and_then(|v| parse_birthday(&v))
        .ok_or(INVALID_BIRTHDAY)?;
    let cpf = field("cpf")
        .and_then(|v| normalize_cpf(&v))
        .ok_or(INVALID_CPF)?;
    Ok(UserInput {
        name,
        email,
        birthday,
        cpf,
    })
}

async fn list_users(State(pool): State<PgPool>) -> Result<Json<Vec<User>>, ApiError> {
    let users = sqlx::query_as(concat!("SELECT ", columns!(), " FROM users ORDER BY id"))
        .fetch_all(&pool)
        .await?;
    Ok(Json(users))
}

async fn get_user(
    State(pool): State<PgPool>,
    path: Result<Path<i64>, PathRejection>,
) -> Result<Json<User>, ApiError> {
    let id = user_id(path)?;
    sqlx::query_as(concat!("SELECT ", columns!(), " FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(&pool)
        .await?
        .map(Json)
        .ok_or(NOT_FOUND)
}

async fn create_user(
    State(pool): State<PgPool>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let input = user_input(body)?;
    let user = sqlx::query_as(concat!(
        "INSERT INTO users (name, email, birthday, cpf) VALUES ($1, $2, $3, $4) RETURNING ",
        columns!()
    ))
    .bind(input.name)
    .bind(input.email)
    .bind(input.birthday)
    .bind(input.cpf)
    .fetch_one(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_user(
    State(pool): State<PgPool>,
    path: Result<Path<i64>, PathRejection>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<User>, ApiError> {
    let id = user_id(path)?;
    let input = user_input(body)?;
    sqlx::query_as(concat!(
        "UPDATE users SET name = $1, email = $2, birthday = $3, cpf = $4, updated_at = now() \
        WHERE id = $5 RETURNING ",
        columns!()
    ))
    .bind(input.name)
    .bind(input.email)
    .bind(input.birthday)
    .bind(input.cpf)
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
