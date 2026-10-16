# REST APIs Ubiquitous

One **Users CRUD REST API**, written four times in four languages. All four are backed by
**one shared PostgreSQL database**, with **pgAdmin** pre-configured to browse it:

| Service          | What                          | Postgres driver                                   | Host port | Directory                            |
| ---------------- | ----------------------------- | ------------------------------------------------- | --------- | ------------------------------------ |
| `python-fastapi` | Python, [FastAPI](https://fastapi.tiangolo.com/)       | [psycopg 3](https://www.psycopg.org/psycopg3/) + pool | `8001` | [`python-fastapi/`](python-fastapi/) |
| `node-express`   | Node.js, [Express 5](https://expressjs.com/)           | [node-postgres (`pg`)](https://node-postgres.com/)    | `8002` | [`node-express/`](node-express/)     |
| `rust-axum`      | Rust, [Axum](https://github.com/tokio-rs/axum) + Tokio | [SQLx](https://github.com/launchbadge/sqlx)           | `8003` | [`rust-axum/`](rust-axum/)           |
| `go-gin`         | Go, [Gin](https://gin-gonic.com/)                      | [pgx v5](https://github.com/jackc/pgx)                | `8004` | [`go-gin/`](go-gin/)                 |
| `postgres`       | PostgreSQL 18 (database `users`)                       | –                                                     | `5432` | [`postgres/`](postgres/)             |
| `pgadmin`        | pgAdmin 4, server pre-registered                       | –                                                     | `5050` | [`pgadmin/`](pgadmin/)               |

Every service is built from **its own Dockerfile**, and **one `docker-compose.yml`** starts them all.

**All four APIs have the same endpoints, and they accept the same requests and return the same
responses, byte for byte.** That includes every validation error. [`test.sh`](test.sh) checks
this by sending the same requests to all four and comparing the answers.

```
  :8001 python-fastapi ─┐
  :8002 node-express   ─┤
  :8003 rust-axum      ─┼──►  postgres:5432  ──  database "users", table "users"
  :8004 go-gin         ─┘           ▲
                                    │
  :5050 pgadmin ────────────────────┘
```

---

## Quick start

Requirements: [Docker](https://docs.docker.com/get-docker/) with the Compose plugin.

```bash
# Build and start everything
docker compose up --build -d

# Run the smoke test against all four APIs
./test.sh

# Stop (the data stays in the "pgdata" volume)
docker compose down

# Stop and delete the database
docker compose down -v
```

Once it's running:

| URL                                   | What                                                           |
| ------------------------------------- | -------------------------------------------------------------- |
| <http://localhost:8001/users>         | Python / FastAPI (interactive docs at <http://localhost:8001/docs>) |
| <http://localhost:8002/users>         | Node / Express                                                 |
| <http://localhost:8003/users>         | Rust / Axum                                                    |
| <http://localhost:8004/users>         | Go / Gin                                                       |
| <http://localhost:5050>               | pgAdmin (no login; open **Servers → users (docker compose)**)  |
| `localhost:5432`                      | Postgres: database `users`, user `api`, password `api`         |

> **Upgrading from an older version of this repo?** The schema lives in
> [`postgres/init.sql`](postgres/init.sql), and Postgres only runs it when the data volume is
> empty. There are no migrations, so recreate the database once:
> `docker compose down -v && docker compose up --build -d`.

---

## API contract

This section is the specification all four APIs implement.

### The `User` resource

```json
{
  "id": 1,
  "name": "Ada Lovelace",
  "email": "ada@example.com",
  "birthday": "1990-05-17",
  "cpf": "52998224725",
  "inserted_at": "2026-10-16T00:09:21.746Z",
  "updated_at": "2026-10-16T00:09:21.746Z"
}
```

| Field         | Type    | Set by  | Format and rules                                                                    |
| ------------- | ------- | ------- | ----------------------------------------------------------------------------------- |
| `id`          | integer | server  | Assigned by Postgres, increasing, never reused (there can be gaps)                  |
| `name`        | string  | client  | Required, non-empty                                                                 |
| `email`       | string  | client  | Required, exactly one `@` with text on both sides, **unique**                       |
| `birthday`    | string  | client  | Required, `YYYY-MM-DD`, a real calendar date between `1900-01-01` and today (UTC)   |
| `cpf`         | string  | client  | Required, a valid [CPF](https://pt.wikipedia.org/wiki/Cadastro_de_Pessoas_F%C3%ADsicas), **unique**. Sent as `52998224725` or `529.982.247-25`; always returned as 11 digits |
| `inserted_at` | string  | server  | ISO 8601 in UTC with milliseconds (`YYYY-MM-DDTHH:MM:SS.sssZ`), set on create       |
| `updated_at`  | string  | server  | Same format; equal to `inserted_at` on create, refreshed on every `PUT`             |

The fields always appear in this order.

A CPF is valid when both verification digits match and its digits are not all equal
(`111.111.111-11` is rejected). A partial mask such as `529.982.24725` is rejected.

### Endpoints

| Method   | Path          | Request body       | Success                                  | Errors                 |
| -------- | ------------- | ------------------ | ---------------------------------------- | ---------------------- |
| `GET`    | `/users`      | –                  | `200` array of users, ordered by `id`    | –                      |
| `GET`    | `/users/{id}` | –                  | `200` the user                           | `400`, `404`           |
| `POST`   | `/users`      | user input         | `201` the created user                   | `400`, `409`           |
| `PUT`    | `/users/{id}` | user input         | `200` the updated user                   | `400`, `404`, `409`    |
| `DELETE` | `/users/{id}` | –                  | `204` empty body                         | `400`, `404`           |

Any endpoint can also return `500` if the database is unavailable.

The **user input** for `POST` and `PUT` is a JSON object with exactly the client fields. `PUT`
is a full replacement, so all four fields are required:

```json
{ "name": "Ada Lovelace", "email": "ada@example.com", "birthday": "1990-05-17", "cpf": "529.982.247-25" }
```

The request must have `Content-Type: application/json` (a `charset` parameter is fine). Unknown
fields are ignored. `id`, `inserted_at` and `updated_at` cannot be set by the client.

### Errors

Every error has the same body, `{"error": "<message>"}`, with one of these exact messages:

| Status | `error`                                                               | When                                                                     |
| ------ | --------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `400`  | `invalid id`                                                          | `{id}` is not a positive integer that fits in a `BIGINT` (`abc`, `0`, `-1`, `99999999999999999999`) |
| `400`  | `invalid request body`                                                | Content type isn't JSON, body is missing or malformed, or isn't a JSON object |
| `400`  | `name must be a non-empty string`                                     | `name` is missing, not a string, or empty                                |
| `400`  | `email must be a valid email address`                                 | `email` is missing, not a string, or doesn't have exactly one `@` with text on both sides |
| `400`  | `birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today`   | `birthday` is missing, not a string, malformed (`1990-5-17`), impossible (`1990-02-30`), or out of range |
| `400`  | `cpf must be a valid CPF`                                             | `cpf` is missing, not a string, in the wrong format, or has wrong verification digits |
| `404`  | `user not found`                                                      | No user has that `id`                                                    |
| `409`  | `email already registered`                                            | Another user already has this email                                      |
| `409`  | `cpf already registered`                                              | Another user already has this CPF                                        |
| `500`  | `internal server error`                                               | A database error (e.g. Postgres is down). The details are logged, not returned. |

When a request has several problems, only the first one is reported, in this order:

1. `invalid id`
2. `invalid request body`
3. `name`, then `email`, then `birthday`, then `cpf`
4. `user not found`
5. `already registered`

### Examples

Replace the port with any of `8001`–`8004`. The responses are identical.

```bash
# Create (the CPF mask is accepted and normalised)
curl -i -X POST localhost:8001/users \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada Lovelace","email":"ada@example.com","birthday":"1990-05-17","cpf":"529.982.247-25"}'
# HTTP/1.1 201 Created
# {"id":1,"name":"Ada Lovelace","email":"ada@example.com","birthday":"1990-05-17","cpf":"52998224725","inserted_at":"2026-10-16T00:09:21.746Z","updated_at":"2026-10-16T00:09:21.746Z"}

# List (from a different API: same database)
curl localhost:8004/users

# Read one
curl localhost:8002/users/1

# Update (full replacement; updated_at changes)
curl -X PUT localhost:8003/users/1 \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada King","email":"ada@example.com","birthday":"1990-05-17","cpf":"52998224725"}'

# Delete
curl -i -X DELETE localhost:8001/users/1
# HTTP/1.1 204 No Content

# Errors
curl localhost:8001/users/abc
# {"error":"invalid id"}
curl -X POST localhost:8001/users -H 'Content-Type: application/json' \
  -d '{"name":"Bob","email":"bob@example.com","birthday":"1990-02-30","cpf":"52998224725"}'
# {"error":"birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today"}
```

---

## Database

### Schema

[`postgres/init.sql`](postgres/init.sql) is the only definition of the schema. None of the
APIs create or change tables.

```sql
CREATE TABLE users (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name        TEXT        NOT NULL,
    email       TEXT        NOT NULL CONSTRAINT users_email_key UNIQUE,
    birthday    DATE        NOT NULL,
    cpf         CHAR(11)    NOT NULL CONSTRAINT users_cpf_key UNIQUE,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- **Uniqueness:** Postgres enforces it through the two named constraints. Each API catches the
  unique-violation error (SQLSTATE `23505`) and uses the constraint name to pick the `409`
  message. That keeps the check race-free, unlike checking first and inserting after.
- **Timestamps:** `inserted_at` comes from the column default. `updated_at` is set to `now()` by
  the `UPDATE` statement in each API.
- **ID gaps:** a failed insert (such as a duplicate CPF) still uses up an identity value, so IDs
  can have gaps. This is normal Postgres behaviour.

### Why Postgres formats dates and timestamps

Each language serialises dates differently. JavaScript has millisecond precision, Go and Rust
trim trailing zeros, and Python prints microseconds. So every API selects the same column list,
and Postgres does the formatting:

```sql
id, name, email, to_char(birthday, 'YYYY-MM-DD') AS birthday, cpf,
to_char(inserted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS inserted_at,
to_char(updated_at  AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at
```

The APIs receive ready-made strings and pass them through untouched, which guarantees identical
output. Writes use `RETURNING` with the same column list, so the created or updated user comes
back in one round trip.

### Postgres image ([`postgres/`](postgres/))

The [`Dockerfile`](postgres/Dockerfile) extends `postgres:18-alpine`:

- It sets the database, user and password (`users` / `api` / `api`).
- It copies `init.sql` into `/docker-entrypoint-initdb.d/`.
- It defines a `HEALTHCHECK` that the APIs and pgAdmin wait on.
  The check uses `pg_isready` over TCP rather than the Unix socket. While `init.sql` runs,
  Postgres uses a temporary server that only listens on the socket, so a socket check would
  report "ready" too early.

Every API reads its connection string from `DATABASE_URL`. Compose sets it to
`postgres://api:api@postgres:5432/users`. When it's unset, the APIs default to
`postgres://api:api@localhost:5432/users`, which lets you run an API on your machine against
the containerised database.

> The credentials are demo values. If you change them, update them in all three places:
> `postgres/Dockerfile`, `DATABASE_URL` in `docker-compose.yml`, and `pgadmin/pgpass`.

---

## pgAdmin ([`pgadmin/`](pgadmin/))

Open <http://localhost:5050>. There is no login. Expand **Servers → users (docker compose)**
and it connects straight away, with no password prompt, so you can browse
**Databases → users → Schemas → public → Tables → users**.

The [`Dockerfile`](pgadmin/Dockerfile) extends `dpage/pgadmin4:9.18` and sets it up as follows:

| What                     | How                                                                                       |
| ------------------------ | ----------------------------------------------------------------------------------------- |
| No pgAdmin login         | `PGADMIN_CONFIG_SERVER_MODE=False` (desktop mode) and `PGADMIN_CONFIG_MASTER_PASSWORD_REQUIRED=False` |
| Server pre-registered    | [`servers.json`](pgadmin/servers.json) is copied to `/pgadmin4/servers.json`. pgAdmin imports it on first start. |
| No database password prompt | [`pgpass`](pgadmin/pgpass) is copied to `/pgpass`, owned by the `pgadmin` user with mode `600`. libpq ignores password files that other users can read, which is why it's baked into the image instead of bind-mounted. |
| Works without IPv6       | `PGADMIN_LISTEN_ADDRESS=0.0.0.0`. pgAdmin's default, `::`, crashes on hosts with IPv6 disabled. |
| Fast `docker compose stop` | `STOPSIGNAL SIGINT`. pgAdmin's gunicorn treats `SIGTERM` as a graceful shutdown that outlasts Docker's 10-second timeout, while `SIGINT` stops it in about a second. |

> Desktop mode means anyone who can reach port `5050` gets full access to the database. That's
> fine on your own machine, but don't expose it on a shared network.

---

## Implementations

All four follow the same shape:

1. Create a connection pool at startup.
2. Validate the `id`, then the body, field by field, in the order of the contract.
3. Run one SQL statement with bound parameters (never string concatenation of input).
4. Map driver errors: unique violation to `409`, anything else to `500`.

Each API is a single source file.

### Python: FastAPI ([`python-fastapi/`](python-fastapi/))

| File               | Purpose                                                          |
| ------------------ | ---------------------------------------------------------------- |
| `main.py`          | The whole app: models, validators, routes, error handlers, pool  |
| `requirements.txt` | Pinned `fastapi`, `uvicorn`, `psycopg[binary]`, `psycopg-pool`   |
| `Dockerfile`       | `python:3.13-slim`, runs as `nobody`                             |

- **Validation:** the `UserIn` Pydantic model attaches an `AfterValidator` to each field
  (`check_email`, `check_birthday`, `normalize_cpf`).
  - FastAPI's default `422` response is replaced by a `RequestValidationError` handler. It
    reports the first failing field with the contract's message.
  - A small `require_json` dependency rejects non-JSON content types, because FastAPI would
    otherwise parse a body sent with no `Content-Type` at all.
- **Database:** psycopg's `AsyncConnectionPool`, opened in FastAPI's `lifespan`, with
  `check=AsyncConnectionPool.check_connection` so it recovers after a Postgres restart. Rows map
  straight into the `User` model with `class_row`. A `UniqueViolation` exception handler
  produces the `409`s.
- **Extras:** OpenAPI docs at `/docs` and `/redoc`.

```bash
cd python-fastapi && python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt && PORT=8001 python main.py
```

### Node.js: Express ([`node-express/`](node-express/))

| File                | Purpose                                   |
| ------------------- | ----------------------------------------- |
| `index.js`          | The whole app                             |
| `package.json`      | Depends on `express@^5` and `pg@^8`       |
| `package-lock.json` | Locked dependency tree, used by `npm ci`  |
| `Dockerfile`        | `node:24-alpine`, runs as the `node` user |

- **Validation:** `parseInput()` returns either `{ input }` or `{ error }`. `app.param("id")`
  validates the id once for every route, using `BigInt` so the full `BIGINT` range works.
  `express.json()` only parses `application/json`, so any other content type leaves
  `req.body` undefined and becomes `invalid request body`.
- **Database:** a `pg.Pool`. Two node-postgres details are handled explicitly:
  - `BIGINT` ids would arrive as strings, so a type parser converts them to numbers.
  - The pool gets an `error` listener. Without one, a dropped idle connection crashes the
    process.
- **Errors:** Express 5 forwards rejected promises from `async` handlers to one error
  middleware. It turns malformed JSON into `400`, error code `23505` into `409`, and anything
  else into `500`.

```bash
cd node-express && npm install && PORT=8002 npm start
```

### Rust: Axum ([`rust-axum/`](rust-axum/))

| File          | Purpose                                                                     |
| ------------- | --------------------------------------------------------------------------- |
| `src/main.rs` | The whole app                                                               |
| `Cargo.toml`  | `axum`, `tokio`, `serde`, `serde_json`, `chrono`, `sqlx` (Postgres + chrono only) |
| `Cargo.lock`  | Locked dependency tree, used by `cargo build --locked`                      |
| `Dockerfile`  | Multi-stage: `rust:1-slim-bookworm` → `distroless/cc-debian12:nonroot`      |

- **Validation:**
  - The body is extracted as `Result<Json<Value>, JsonRejection>`, then each field is read
    and checked in order. Axum's own rejections (wrong content type, bad JSON) become
    `invalid request body`.
  - The birthday is parsed with `chrono::NaiveDate`, plus an explicit `YYYY-MM-DD` shape
    check, because chrono alone would also accept unpadded months.
- **Database:**
  - A `sqlx::PgPool` is passed to the handlers as Axum state.
  - Every query is a compile-time `&'static str`, built with `concat!` from a `columns!()`
    macro. SQLx 0.9 refuses SQL built at runtime with `format!`.
  - `ApiError` implements `From<sqlx::Error>`, so every database call can use `?`. Unique
    violations become `409` there.
- **Image:** only the compiled binary on a distroless base, with no shell and no package
  manager.

```bash
cd rust-axum && PORT=8003 cargo run --release
```

### Go: Gin ([`go-gin/`](go-gin/))

| File         | Purpose                                                              |
| ------------ | -------------------------------------------------------------------- |
| `main.go`    | The whole app                                                        |
| `go.mod`     | Requires Go ≥ 1.25, Gin v1.12, pgx v5                                |
| `go.sum`     | Dependency checksums                                                 |
| `Dockerfile` | Multi-stage: `golang:1.27-alpine` → `distroless/static-debian12:nonroot` |

- **Validation:**
  - The body is bound into a `map[string]any`, so a field with the wrong type fails as that
    field, not as the whole body. `validateInput()` then checks the fields in order.
  - The content type is checked explicitly, because Gin's `ShouldBindJSON` would accept any.
  - `time.Parse(time.DateOnly, ...)` handles birthdays. It is strict about padding and
    rejects impossible dates.
- **Database:** a `pgxpool.Pool`.
  - The `respondWithUser` helper runs any single-row query (select, insert, update) with
    `pgx.CollectOneRow`, and maps `pgx.ErrNoRows` to `404` and `*pgconn.PgError` code
    `23505` to `409`.
  - Queries use the request's context, so they are cancelled if the client disconnects.
- **Image:** a static (`CGO_ENABLED=0`) binary on a distroless base, with `GIN_MODE=release`.

```bash
cd go-gin && PORT=8004 go run .
```

---

## Docker Compose

[`docker-compose.yml`](docker-compose.yml) defines six services, each built from its own
directory's `Dockerfile`:

| Service          | Build            | Ports       | Notes                                              |
| ---------------- | ---------------- | ----------- | -------------------------------------------------- |
| `postgres`       | `./postgres`     | `5432:5432` | Data in the named volume `pgdata`                  |
| `pgadmin`        | `./pgadmin`      | `5050:80`   | Starts once Postgres is healthy                    |
| `python-fastapi` | `./python-fastapi` | `8001:8080` | Shared `x-api` settings                          |
| `node-express`   | `./node-express` | `8002:8080` | Shared `x-api` settings                            |
| `rust-axum`      | `./rust-axum`    | `8003:8080` | Shared `x-api` settings                            |
| `go-gin`         | `./go-gin`       | `8004:8080` | Shared `x-api` settings                            |

The four APIs share one block of settings through a YAML anchor:

```yaml
x-api: &api
  init: true
  environment:
    DATABASE_URL: postgres://api:api@postgres:5432/users
  depends_on:
    postgres:
      condition: service_healthy
```

- **`depends_on`** waits for the Postgres image's health check, so no API starts before the
  schema exists.
- **`init: true`** runs a tiny init process as PID 1. Node and the Rust binary don't handle
  `SIGTERM` when they run as PID 1, so without it, `docker compose stop` would hang for 10
  seconds before killing them.
- **Non-root:** every API container runs as a non-root user.

---

## Smoke test

[`test.sh`](test.sh) needs only `bash` and `curl`. It waits for each API to come up, then runs
two parts:

1. **The CRUD scenario against each API on its own.** It creates, lists, reads and updates a
   user, then tries an empty name, a bad email, an impossible birthday, a bad CPF check digit,
   a duplicate CPF, a duplicate email and a bad id. Finally it deletes the user, then reads and
   deletes it again to check for `404`.
2. **Identical responses across all four APIs.**
   - It creates a user with a masked CPF and checks it comes back as 11 digits.
   - It sends about 25 requests (valid reads plus every kind of invalid input and edge-case
     id) to **all four** APIs. Each check fails unless all four return the same status code
     and a byte-identical body.
   - It updates the user through Go, checks `updated_at` moved forward, and deletes it
     through Rust.

It generates random valid CPFs and emails, and deletes what it creates, so it can be run any
number of times against the same database. It exits non-zero on any failure.

```bash
./test.sh                  # against localhost
HOST=192.168.1.10 ./test.sh
```

```
identical responses (shared database)
  ok    POST   /users         201  create with masked cpf 822.310.588-80
  ok    cpf is stored as 11 digits
  ok    read the same user                       200 {"id":13,"name":"Grace Hopper",...
  ok    body is not JSON                         400 {"error":"invalid request body"}
  ok    birthday in the future                   400 {"error":"birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today"}
  ok    duplicate cpf                            409 {"error":"cpf already registered"}
  ...
all checks passed
```

---

## Project layout

```
.
├── docker-compose.yml
├── test.sh
├── postgres/
│   ├── Dockerfile
│   └── init.sql
├── pgadmin/
│   ├── Dockerfile
│   ├── servers.json
│   └── pgpass
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

- No authentication, pagination, filtering, or `PATCH` (partial updates).
- No migrations. The schema is created once by `postgres/init.sql`, so schema changes need
  `docker compose down -v`.
- Email checking is intentionally loose: one `@` with text on both sides.
- Demo credentials are baked into the Postgres and pgAdmin images, and pgAdmin has no login.
  This setup is for local development only.
