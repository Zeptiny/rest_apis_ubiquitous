# REST APIs Ubiquitous

One tiny **Users CRUD REST API**, written four times in four languages, all backed by
**one shared PostgreSQL database**:

| Service          | Language   | Framework                                         | Postgres driver                                     | Host port | Directory                            |
| ---------------- | ---------- | ------------------------------------------------- | --------------------------------------------------- | --------- | ------------------------------------ |
| `python-fastapi` | Python     | [FastAPI](https://fastapi.tiangolo.com/)          | [psycopg 3](https://www.psycopg.org/psycopg3/) + pool | `8001`  | [`python-fastapi/`](python-fastapi/) |
| `node-express`   | Node.js    | [Express 5](https://expressjs.com/)               | [node-postgres (`pg`)](https://node-postgres.com/)  | `8002`    | [`node-express/`](node-express/)     |
| `rust-axum`      | Rust       | [Axum](https://github.com/tokio-rs/axum) + Tokio  | [SQLx](https://github.com/launchbadge/sqlx)         | `8003`    | [`rust-axum/`](rust-axum/)           |
| `go-gin`         | Go         | [Gin](https://gin-gonic.com/)                     | [pgx v5](https://github.com/jackc/pgx)              | `8004`    | [`go-gin/`](go-gin/)                 |
| `postgres`       | –          | PostgreSQL 18                                     | –                                                   | `5432`    | [`db/`](db/)                         |

All four implement **exactly the same contract**: same routes, same JSON, same status codes,
same error messages. They all read and write the **same `users` table**, so a user created
through one API is immediately visible through the other three. That makes them a good
side-by-side comparison of how each language and framework handles routing, validation, and
database access.

```
  :8001 python-fastapi ─┐
  :8002 node-express   ─┤
  :8003 rust-axum      ─┼──►  postgres:5432  ──  database "users"  ──  table "users"
  :8004 go-gin         ─┘
```

---

## Quick start

Requirements: [Docker](https://docs.docker.com/get-docker/) with the Compose plugin.

```bash
# Build and start Postgres and all four APIs
docker compose up --build -d

# Run the smoke test against all of them
./test.sh

# Stop the containers (data is kept in the "pgdata" volume)
docker compose down

# Stop the containers AND delete the database
docker compose down -v
```

Once the stack is running:

- Python / FastAPI → <http://localhost:8001/users> (interactive docs at <http://localhost:8001/docs>)
- Node / Express → <http://localhost:8002/users>
- Rust / Axum → <http://localhost:8003/users>
- Go / Gin → <http://localhost:8004/users>
- Postgres → `localhost:5432`, database `users`, user `api`, password `api`

To start just one API (Compose starts Postgres for it automatically):
`docker compose up --build rust-axum`.

To open a SQL shell: `docker compose exec postgres psql -U api -d users`.

---

## Database

### Schema

[`db/init.sql`](db/init.sql) is the single source of truth for the schema. None of the APIs
create tables themselves.

```sql
CREATE TABLE IF NOT EXISTS users (
    id    BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name  TEXT NOT NULL,
    email TEXT NOT NULL
);
```

The Postgres image runs it **only once**, when the `pgdata` volume is empty. After changing
it, run `docker compose down -v` to recreate the database.

### Configuration

Every API reads its connection string from the `DATABASE_URL` environment variable:

| Where                         | `DATABASE_URL`                                                   |
| ----------------------------- | ---------------------------------------------------------------- |
| Inside Docker Compose         | `postgres://api:api@postgres:5432/users` (set in `docker-compose.yml`) |
| Default, when the variable is unset | `postgres://api:api@localhost:5432/users` (for running an API on your machine) |

The credentials (`api` / `api`) are demo values. Change them in `docker-compose.yml` before
using this anywhere real.

---

## API reference

All four services expose the same API. Inside its container, each one listens on port `8080`.
You can change that with the `PORT` environment variable.

### The `User` resource

```json
{
  "id": 1,
  "name": "Ada Lovelace",
  "email": "ada@example.com"
}
```

| Field   | Type    | Notes                                                                  |
| ------- | ------- | ---------------------------------------------------------------------- |
| `id`    | integer | Assigned by Postgres (identity column), shared by all four APIs, never reused |
| `name`  | string  | Required, must be non-empty                                            |
| `email` | string  | Required, must be non-empty (format is not checked)                    |

Unknown fields in a request body are ignored.

### Endpoints

| Method   | Path          | Body                | Success          | Errors          |
| -------- | ------------- | ------------------- | ---------------- | --------------- |
| `GET`    | `/users`      | –                   | `200` list of users (sorted by `id`) | –  |
| `GET`    | `/users/{id}` | –                   | `200` the user   | `400`, `404`    |
| `POST`   | `/users`      | `{"name","email"}`  | `201` the created user | `400`     |
| `PUT`    | `/users/{id}` | `{"name","email"}`  | `200` the updated user | `400`, `404` |
| `DELETE` | `/users/{id}` | –                   | `204` no content | `400`, `404`    |

Any endpoint can also return `500` if the database is unavailable. `PUT` is a full
replacement: both `name` and `email` must be sent.

### Errors

Every error has the same shape, `{"error": "<message>"}`:

| Status | Message                         | When                                                                 |
| ------ | ------------------------------- | -------------------------------------------------------------------- |
| `400`  | `invalid id`                    | `{id}` is not a positive integer that fits in a `BIGINT` (e.g. `abc`, `0`, `-1`, `99999999999999999999`) |
| `400`  | `name and email are required`   | Body is missing, isn't valid JSON, or `name`/`email` is missing, empty, or not a string |
| `404`  | `user not found`                | No user with that `id` exists                                        |
| `500`  | `internal server error`         | The database query failed (e.g. Postgres is down). The detail is logged, not returned. |

When a request has more than one problem, the API reports the first one in this order:
invalid id, then invalid body, then user not found.

### Examples

Replace `8001` with any of the four ports. The responses are identical.

```bash
# Create
curl -i -X POST localhost:8001/users \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada","email":"ada@example.com"}'
# HTTP/1.1 201 Created
# {"id":1,"name":"Ada","email":"ada@example.com"}

# List (from a different API: same database)
curl localhost:8004/users
# [{"id":1,"name":"Ada","email":"ada@example.com"}]

# Read one
curl localhost:8002/users/1
# {"id":1,"name":"Ada","email":"ada@example.com"}

# Update (full replacement)
curl -X PUT localhost:8003/users/1 \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada Lovelace","email":"ada@example.com"}'
# {"id":1,"name":"Ada Lovelace","email":"ada@example.com"}

# Delete
curl -i -X DELETE localhost:8001/users/1
# HTTP/1.1 204 No Content

# Errors
curl localhost:8001/users/1     # {"error":"user not found"}
curl localhost:8001/users/abc   # {"error":"invalid id"}
curl -X POST localhost:8001/users -H 'Content-Type: application/json' -d '{"name":""}'
                                # {"error":"name and email are required"}
```

---

## Implementations

All four share the same structure: a connection pool created at startup, five route handlers
that each run a single SQL statement, and the same validation rules. The details below cover
what differs.

All writes use `RETURNING id, name, email`, so the API returns the row exactly as Postgres
stored it, without a second query. Every query uses bound parameters (`$1` / `%s`), never
string concatenation.

### Python: FastAPI ([`python-fastapi/`](python-fastapi/))

| File               | Purpose                                                  |
| ------------------ | -------------------------------------------------------- |
| `main.py`          | The whole app: models, routes, error handlers, DB pool   |
| `requirements.txt` | Pinned `fastapi`, `uvicorn`, `psycopg[binary]`, `psycopg-pool` |
| `Dockerfile`       | `python:3.13-slim`, runs as `nobody`                     |

- **Database:** an `AsyncConnectionPool` from `psycopg_pool`, opened and closed in FastAPI's
  `lifespan`. A small `query()` helper maps rows straight into the `User` Pydantic model
  with psycopg's `class_row`. The pool is created with
  `check=AsyncConnectionPool.check_connection`, so connections broken by a Postgres restart
  are replaced instead of failing a request.
- **Validation:** Pydantic models (`UserIn` with `Field(min_length=1)`) and a typed path
  parameter (`Annotated[int, Path(gt=0, lt=2**63)]`). FastAPI returns `422` with a detailed
  body by default. A custom `RequestValidationError` handler turns that into the shared `400`
  format.
- **Extras:** free OpenAPI docs at `/docs` and `/redoc`.

Run without Docker (with Postgres running from `docker compose up -d postgres`):

```bash
cd python-fastapi
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
PORT=8001 python main.py
```

### Node.js: Express ([`node-express/`](node-express/))

| File                | Purpose                                   |
| ------------------- | ----------------------------------------- |
| `index.js`          | The whole app                             |
| `package.json`      | Depends on `express@^5` and `pg@^8`       |
| `package-lock.json` | Locked dependency tree, used by `npm ci`  |
| `Dockerfile`        | `node:24-alpine`, runs as the `node` user |

- **Database:** a `pg.Pool`. Two node-postgres details are handled explicitly:
  - `BIGINT` columns arrive as strings by default, so a type parser converts them back to numbers.
  - The pool gets an `error` listener. Without one, an idle connection dropped by Postgres
    raises an unhandled `error` event and crashes the process.
- **Validation:** an `app.param("id", ...)` hook validates `:id` once for every route that has
  one. It uses `BigInt` so the full `BIGINT` range is accepted. A small `parseInput()` helper
  checks the body.
- **Errors:** Express 5 forwards rejected promises from `async` handlers to the error
  middleware. That middleware turns malformed JSON into a `400` and anything else into a `500`.

Run without Docker:

```bash
cd node-express
npm install
PORT=8002 npm start
```

### Rust: Axum ([`rust-axum/`](rust-axum/))

| File          | Purpose                                                        |
| ------------- | -------------------------------------------------------------- |
| `src/main.rs` | The whole app                                                  |
| `Cargo.toml`  | `axum`, `tokio`, `serde`, `serde_json`, `sqlx` (Postgres only, no default features) |
| `Cargo.lock`  | Locked dependency tree, used by `cargo build --locked`         |
| `Dockerfile`  | Multi-stage: `rust:1-slim-bookworm` → `distroless/cc-debian12:nonroot` |

- **Database:** a `sqlx::PgPool`, passed to the handlers as Axum state. Rows map onto `User`
  with `#[derive(FromRow)]`. The queries use `sqlx::query_as` (checked at runtime), so
  building the image doesn't need a live database.
- **Validation:** the handlers take `Result<Path<i64>, PathRejection>` and
  `Result<Json<UserInput>, JsonRejection>` instead of the plain extractors. That way extraction
  failures come back in the shared error format instead of Axum's plain-text rejections.
- **Errors:** a small `ApiError` type implements `IntoResponse` and `From<sqlx::Error>`, so
  every database call can simply use `?`.
- **Image:** the final image holds only the compiled binary on a distroless base, with no
  shell and no package manager.

Run without Docker:

```bash
cd rust-axum
PORT=8003 cargo run --release
```

### Go: Gin ([`go-gin/`](go-gin/))

| File         | Purpose                                                              |
| ------------ | -------------------------------------------------------------------- |
| `main.go`    | The whole app                                                        |
| `go.mod`     | Module definition, requires Go ≥ 1.25, Gin v1.12, and pgx v5         |
| `go.sum`     | Dependency checksums                                                 |
| `Dockerfile` | Multi-stage: `golang:1.27-alpine` → `distroless/static-debian12:nonroot` |

- **Database:** a `pgxpool.Pool`. A `respondWithUser` helper runs any query that returns at
  most one user (select, insert, update) and maps `pgx.ErrNoRows` to `404`. The list endpoint
  uses `pgx.CollectRows` with `pgx.RowToStructByPos[User]`. Queries use the request's
  context, so they are cancelled if the client disconnects.
- **Validation:** `c.ShouldBindJSON` with `binding:"required"` struct tags (which reject
  missing or empty strings). `strconv.ParseInt(..., 64)` parses the `:id` parameter.
- **Image:** a static (`CGO_ENABLED=0`) binary on a distroless base, with `GIN_MODE=release`.

Run without Docker:

```bash
cd go-gin
PORT=8004 go run .
```

---

## Docker Compose

[`docker-compose.yml`](docker-compose.yml) defines five services:

- **`postgres`** (`postgres:18-alpine`):
  - creates the `users` database and the `api` user;
  - runs `db/init.sql` on first start;
  - stores its data in the named volume `pgdata`;
  - publishes port `5432` so you can connect from your machine.
- **The four APIs** each build their own directory's `Dockerfile` and map container port
  `8080` to host ports `8001`–`8004`. They share one block of settings through a YAML anchor
  (`x-api`):

  ```yaml
  x-api: &api
    init: true
    environment:
      DATABASE_URL: postgres://api:api@postgres:5432/users
    depends_on:
      postgres:
        condition: service_healthy
  ```

Some details:

- **Start order:** Postgres has a `pg_isready` health check, and every API waits for it to be
  healthy before starting. The check connects over TCP on purpose. While `init.sql` runs,
  Postgres uses a temporary server that only listens on a Unix socket, so a socket check would
  report "ready" too early.
- **`init: true`** runs a tiny init process as PID 1. Node and the Rust binary don't handle
  `SIGTERM` when they run as PID 1, so without it, `docker compose stop` would hang for 10
  seconds before killing them.
- **Non-root:** all API containers run as a non-root user.

---

## Smoke test

[`test.sh`](test.sh) needs only `bash` and `curl`. It waits for each API to come up, then runs:

1. **The same scenario against each API:** create, list, read, update, bad input, bad id,
   delete, then read and delete again to check for `404`.
2. **A shared-database check:** it creates a user on `8001`, reads it on `8002`, updates it on
   `8003`, deletes it on `8004`, and confirms it's gone on `8001`.

It exits non-zero if any status code is wrong. It cleans up after itself, so it can be run
repeatedly against the same database.

```bash
./test.sh                  # against localhost
HOST=192.168.1.10 ./test.sh
```

```
python-fastapi (http://localhost:8001)
  ok    POST   /users         -> 201
  ok    GET    /users         -> 200
  ok    GET    /users/1       -> 200
  ...
shared database
  ok    POST   /users         -> 201
  ok    GET    /users/5       -> 200
  ok    PUT    /users/5       -> 200
  ok    DELETE /users/5       -> 204
  ok    GET    /users/5       -> 404

all checks passed
```

---

## Project layout

```
.
├── docker-compose.yml
├── test.sh
├── db/
│   └── init.sql
├── python-fastapi/
│   ├── Dockerfile
│   ├── main.py
│   └── requirements.txt
├── node-express/
│   ├── Dockerfile
│   ├── index.js
│   ├── package.json
│   └── package-lock.json
├── rust-axum/
│   ├── Dockerfile
│   ├── Cargo.toml
│   ├── Cargo.lock
│   └── src/main.rs
└── go-gin/
    ├── Dockerfile
    ├── go.mod
    ├── go.sum
    └── main.go
```

## Limitations

These are deliberately minimal demo services:

- No authentication, pagination, email-format validation, or uniqueness checks.
- No migration tool: the schema is created once by `db/init.sql`. Changing it means
  recreating the volume (`docker compose down -v`) or running the `ALTER` statements yourself.
- Hard-coded demo database credentials in `docker-compose.yml`.
