# REST APIs Ubiquitous

One tiny **Users CRUD REST API**, written four times in four languages:

| Service          | Language   | Framework                                         | Host port | Directory                            |
| ---------------- | ---------- | ------------------------------------------------- | --------- | ------------------------------------ |
| `python-fastapi` | Python     | [FastAPI](https://fastapi.tiangolo.com/)          | `8001`    | [`python-fastapi/`](python-fastapi/) |
| `node-express`   | Node.js    | [Express 5](https://expressjs.com/)               | `8002`    | [`node-express/`](node-express/)     |
| `rust-axum`      | Rust       | [Axum](https://github.com/tokio-rs/axum) + Tokio  | `8003`    | [`rust-axum/`](rust-axum/)           |
| `go-gin`         | Go         | [Gin](https://gin-gonic.com/)                     | `8004`    | [`go-gin/`](go-gin/)                 |

All four implement **exactly the same contract**: same routes, same JSON, same status codes,
same error messages. Each keeps its data **in memory**, so they make a good side-by-side
comparison of how each language and framework handles routing, validation, and shared state.

---

## Quick start

Requirements: [Docker](https://docs.docker.com/get-docker/) with the Compose plugin.

```bash
# Build and start all four APIs
docker compose up --build -d

# Run the smoke test against all of them
./test.sh

# Stop and remove the containers
docker compose down
```

Once the stack is running:

- Python / FastAPI → <http://localhost:8001/users> (interactive docs at <http://localhost:8001/docs>)
- Node / Express → <http://localhost:8002/users>
- Rust / Axum → <http://localhost:8003/users>
- Go / Gin → <http://localhost:8004/users>

To start only one service: `docker compose up --build rust-axum`.

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

| Field   | Type    | Notes                                                       |
| ------- | ------- | ----------------------------------------------------------- |
| `id`    | integer | Assigned by the server, auto-incremented from `1`, never reused |
| `name`  | string  | Required, must be non-empty                                 |
| `email` | string  | Required, must be non-empty (format is not checked)         |

Unknown fields in a request body are ignored.

### Endpoints

| Method   | Path          | Body                | Success          | Errors          |
| -------- | ------------- | ------------------- | ---------------- | --------------- |
| `GET`    | `/users`      | –                   | `200` list of users (sorted by `id`) | –  |
| `GET`    | `/users/{id}` | –                   | `200` the user   | `400`, `404`    |
| `POST`   | `/users`      | `{"name","email"}`  | `201` the created user | `400`     |
| `PUT`    | `/users/{id}` | `{"name","email"}`  | `200` the updated user | `400`, `404` |
| `DELETE` | `/users/{id}` | –                   | `204` no content | `400`, `404`    |

`PUT` is a full replacement: both `name` and `email` must be sent.

### Errors

Every error has the same shape, `{"error": "<message>"}`:

| Status | Message                         | When                                                                 |
| ------ | ------------------------------- | -------------------------------------------------------------------- |
| `400`  | `invalid id`                    | `{id}` is not a positive integer (e.g. `abc`, `0`, `-1`)             |
| `400`  | `name and email are required`   | Body is missing, isn't valid JSON, or `name`/`email` is missing, empty, or not a string |
| `404`  | `user not found`                | No user with that `id` exists                                        |

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

# List
curl localhost:8001/users
# [{"id":1,"name":"Ada","email":"ada@example.com"}]

# Read one
curl localhost:8001/users/1
# {"id":1,"name":"Ada","email":"ada@example.com"}

# Update (full replacement)
curl -X PUT localhost:8001/users/1 \
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

All four share the same structure: an in-memory map from `id` to user, a counter for the next
`id`, and five route handlers. The details below cover what differs.

### Python: FastAPI ([`python-fastapi/`](python-fastapi/))

| File               | Purpose                                         |
| ------------------ | ----------------------------------------------- |
| `main.py`          | The whole app: models, routes, error handlers   |
| `requirements.txt` | Pinned `fastapi` and `uvicorn`                  |
| `Dockerfile`       | `python:3.13-slim`, runs as `nobody`            |

- **Validation:** Pydantic models (`UserIn` with `Field(min_length=1)`) and a typed path
  parameter (`Annotated[int, Path(gt=0)]`). FastAPI returns `422` with a detailed body by
  default. A custom `RequestValidationError` handler turns that into the shared `400` format.
- **State:** a module-level `dict[int, User]` and `itertools.count(1)` for IDs. The handlers are
  `async def`, so they all run on the single event loop and need no lock.
- **Extras:** free OpenAPI docs at `/docs` and `/redoc`.

Run without Docker:

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
| `package.json`      | Depends on `express@^5`                   |
| `package-lock.json` | Locked dependency tree, used by `npm ci`  |
| `Dockerfile`        | `node:24-alpine`, runs as the `node` user |

- **Validation:** an `app.param("id", ...)` hook validates `:id` once for every route that has
  one. A small `parseInput()` helper checks the body. An error middleware catches malformed JSON
  that `express.json()` rejects.
- **State:** a `Map` (which keeps insertion order, so the list comes back sorted by `id`) and a
  `nextId` counter. Node is single-threaded, so no locking is needed.

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
| `Cargo.toml`  | `axum`, `tokio`, `serde`, `serde_json`                         |
| `Cargo.lock`  | Locked dependency tree, used by `cargo build --locked`         |
| `Dockerfile`  | Multi-stage: `rust:1-slim-bookworm` → `distroless/cc-debian12:nonroot` |

- **Validation:** the handlers take `Result<Path<u64>, PathRejection>` and
  `Result<Json<UserInput>, JsonRejection>` instead of the plain extractors. That way extraction
  failures come back in the shared error format instead of Axum's plain-text rejections.
  Errors are a small `ApiError` type that implements `IntoResponse`.
- **State:** `Arc<RwLock<Store>>`, where `Store` holds a `BTreeMap<u64, User>` (always ordered
  by `id`) and the `next_id` counter. It is shared across Tokio's worker threads via
  `with_state`.
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
| `go.mod`     | Module definition, requires Go ≥ 1.25 and Gin v1.12                  |
| `go.sum`     | Dependency checksums                                                 |
| `Dockerfile` | Multi-stage: `golang:1.27-alpine` → `distroless/static-debian12:nonroot` |

- **Validation:** `c.ShouldBindJSON` with `binding:"required"` struct tags (which reject
  missing or empty strings). `strconv.Atoi` parses the `:id` parameter.
- **State:** a `store` struct with a `map[int]User`, a `nextID` counter, and a
  `sync.RWMutex`, because Gin serves each request on its own goroutine. Go maps have no order,
  so `GET /users` sorts the result by `id`.
- **Image:** a static (`CGO_ENABLED=0`) binary on a distroless base, with `GIN_MODE=release`.

Run without Docker:

```bash
cd go-gin
PORT=8004 go run .
```

---

## Docker Compose

[`docker-compose.yml`](docker-compose.yml) builds each directory's `Dockerfile` and maps
container port `8080` to a different host port. Condensed, it looks like this:

```yaml
services:
  python-fastapi: { build: ./python-fastapi, ports: ["8001:8080"] }
  node-express:   { build: ./node-express,   ports: ["8002:8080"] }
  rust-axum:      { build: ./rust-axum,      ports: ["8003:8080"] }
  go-gin:         { build: ./go-gin,         ports: ["8004:8080"] }
```

Each service also sets `init: true`, which runs a tiny init process as PID 1. Node and the
Rust binary don't handle `SIGTERM` when they run as PID 1, so without it,
`docker compose stop` would hang for 10 seconds before killing them.

All containers run as a non-root user.

Each service is independent: they don't talk to each other or share data. Creating a user on
port `8001` doesn't create it on `8002`.

---

## Smoke test

[`test.sh`](test.sh) runs the same scenario against every API: create, list, read, update,
bad input, bad id, delete, then read and delete again to check for `404`. It exits non-zero if
any status code is wrong. It needs only `bash` and `curl`.

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
all checks passed
```

---

## Project layout

```
.
├── docker-compose.yml
├── test.sh
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

- **Data lives only in memory.** It is lost when a container restarts, and it isn't shared
  between replicas.
- No authentication, pagination, email-format validation, or uniqueness checks.
- No persistence layer, so there are no migrations and no database.
