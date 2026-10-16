import os
import re
from contextlib import asynccontextmanager
from datetime import UTC, date, datetime
from typing import Annotated

import uvicorn
from fastapi import Depends, FastAPI, Header, HTTPException, Path, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from psycopg.errors import UniqueViolation
from psycopg.rows import class_row
from psycopg_pool import AsyncConnectionPool
from pydantic import AfterValidator, BaseModel, Field
from starlette.exceptions import HTTPException as StarletteHTTPException

DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://api:api@localhost:5432/users")

# check= validates each connection on checkout, so the pool recovers after a Postgres restart.
pool = AsyncConnectionPool(DATABASE_URL, check=AsyncConnectionPool.check_connection, open=False)


@asynccontextmanager
async def lifespan(app: FastAPI):
    await pool.open(wait=True)
    yield
    await pool.close()


app = FastAPI(title="Users API (FastAPI)", lifespan=lifespan)

# Dates and timestamps are formatted by Postgres so all four APIs return identical strings.
COLUMNS = """id, name, email, to_char(birthday, 'YYYY-MM-DD') AS birthday, cpf,
    to_char(inserted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS inserted_at,
    to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at"""

INVALID_BODY = "invalid request body"
INVALID_FIELD = {
    "name": "name must be a non-empty string",
    "email": "email must be a valid email address",
    "birthday": "birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today",
    "cpf": "cpf must be a valid CPF",
}

DATE_FORMAT = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}")
CPF_FORMAT = re.compile(r"[0-9]{11}|[0-9]{3}\.[0-9]{3}\.[0-9]{3}-[0-9]{2}")


def check_email(value: str) -> str:
    if value.count("@") != 1 or value.startswith("@") or value.endswith("@"):
        raise ValueError(INVALID_FIELD["email"])
    return value


def check_birthday(value: str) -> str:
    if not DATE_FORMAT.fullmatch(value):
        raise ValueError(INVALID_FIELD["birthday"])
    birthday = date.fromisoformat(value)  # raises ValueError for dates like 2024-02-30
    if not date(1900, 1, 1) <= birthday <= datetime.now(UTC).date():
        raise ValueError(INVALID_FIELD["birthday"])
    return value


def normalize_cpf(value: str) -> str:
    """Accepts 11 digits or the 000.000.000-00 mask, checks both verification digits."""
    if not CPF_FORMAT.fullmatch(value):
        raise ValueError(INVALID_FIELD["cpf"])
    digits = [int(char) for char in value if char.isdigit()]
    if len(set(digits)) == 1:
        raise ValueError(INVALID_FIELD["cpf"])
    for n in (9, 10):
        total = sum(digit * (n + 1 - i) for i, digit in enumerate(digits[:n]))
        if total * 10 % 11 % 10 != digits[n]:
            raise ValueError(INVALID_FIELD["cpf"])
    return "".join(map(str, digits))


class UserIn(BaseModel):
    name: str = Field(min_length=1)
    email: Annotated[str, AfterValidator(check_email)]
    birthday: Annotated[str, AfterValidator(check_birthday)]
    cpf: Annotated[str, AfterValidator(normalize_cpf)]


class User(BaseModel):
    id: int
    name: str
    email: str
    birthday: str
    cpf: str
    inserted_at: str
    updated_at: str


# Must be a positive integer that fits in a Postgres BIGINT.
UserId = Annotated[int, Path(gt=0, lt=2**63)]


def require_json(
    content_type: Annotated[
        str, Header(pattern=r"(?i)^application/json\s*(;|$)", include_in_schema=False)
    ],
) -> None:
    """Request bodies must be sent as application/json, as in the other APIs."""


@app.exception_handler(StarletteHTTPException)
async def http_error(request: Request, exc: StarletteHTTPException) -> JSONResponse:
    return JSONResponse(status_code=exc.status_code, content={"error": exc.detail})


@app.exception_handler(RequestValidationError)
async def validation_error(request: Request, exc: RequestValidationError) -> JSONResponse:
    # Same precedence as the other APIs: id, then content type and body, then each field in order.
    locations = [err["loc"] for err in exc.errors()]
    if any(loc[0] == "path" for loc in locations):
        message = "invalid id"
    elif any(loc[0] == "header" for loc in locations) or len(locations[0]) < 2:
        message = INVALID_BODY
    else:
        message = INVALID_FIELD.get(locations[0][1], INVALID_BODY)
    return JSONResponse(status_code=400, content={"error": message})


@app.exception_handler(UniqueViolation)
async def conflict(request: Request, exc: UniqueViolation) -> JSONResponse:
    field = "cpf" if exc.diag.constraint_name == "users_cpf_key" else "email"
    return JSONResponse(status_code=409, content={"error": f"{field} already registered"})


@app.exception_handler(Exception)
async def server_error(request: Request, exc: Exception) -> JSONResponse:
    return JSONResponse(status_code=500, content={"error": "internal server error"})


async def query(sql: str, *params: object) -> list[User]:
    async with pool.connection() as conn:
        cur = conn.cursor(row_factory=class_row(User))
        await cur.execute(sql, params)
        return await cur.fetchall()


def first_or_404(rows: list[User]) -> User:
    if not rows:
        raise HTTPException(status_code=404, detail="user not found")
    return rows[0]


@app.get("/users")
async def list_users() -> list[User]:
    return await query(f"SELECT {COLUMNS} FROM users ORDER BY id")


@app.get("/users/{user_id}")
async def get_user(user_id: UserId) -> User:
    return first_or_404(await query(f"SELECT {COLUMNS} FROM users WHERE id = %s", user_id))


@app.post("/users", status_code=201, dependencies=[Depends(require_json)])
async def create_user(data: UserIn) -> User:
    rows = await query(
        f"INSERT INTO users (name, email, birthday, cpf) VALUES (%s, %s, %s, %s) RETURNING {COLUMNS}",
        data.name,
        data.email,
        data.birthday,
        data.cpf,
    )
    return rows[0]


@app.put("/users/{user_id}", dependencies=[Depends(require_json)])
async def update_user(user_id: UserId, data: UserIn) -> User:
    rows = await query(
        f"""UPDATE users SET name = %s, email = %s, birthday = %s, cpf = %s, updated_at = now()
        WHERE id = %s RETURNING {COLUMNS}""",
        data.name,
        data.email,
        data.birthday,
        data.cpf,
        user_id,
    )
    return first_or_404(rows)


@app.delete("/users/{user_id}", status_code=204)
async def delete_user(user_id: UserId) -> None:
    first_or_404(await query(f"DELETE FROM users WHERE id = %s RETURNING {COLUMNS}", user_id))


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=int(os.environ.get("PORT", "8080")))
