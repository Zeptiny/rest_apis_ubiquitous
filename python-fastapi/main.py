import os
from contextlib import asynccontextmanager
from typing import Annotated

import uvicorn
from fastapi import FastAPI, HTTPException, Path, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from psycopg.rows import class_row
from psycopg_pool import AsyncConnectionPool
from pydantic import BaseModel, Field
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


class UserIn(BaseModel):
    name: str = Field(min_length=1)
    email: str = Field(min_length=1)


class User(BaseModel):
    id: int
    name: str
    email: str


# Must be a positive integer that fits in a Postgres BIGINT.
UserId = Annotated[int, Path(gt=0, lt=2**63)]


@app.exception_handler(StarletteHTTPException)
async def http_error(request: Request, exc: StarletteHTTPException) -> JSONResponse:
    return JSONResponse(status_code=exc.status_code, content={"error": exc.detail})


@app.exception_handler(RequestValidationError)
async def validation_error(request: Request, exc: RequestValidationError) -> JSONResponse:
    bad_path = any(err["loc"][0] == "path" for err in exc.errors())
    message = "invalid id" if bad_path else "name and email are required"
    return JSONResponse(status_code=400, content={"error": message})


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
    return await query("SELECT id, name, email FROM users ORDER BY id")


@app.get("/users/{user_id}")
async def get_user(user_id: UserId) -> User:
    return first_or_404(await query("SELECT id, name, email FROM users WHERE id = %s", user_id))


@app.post("/users", status_code=201)
async def create_user(data: UserIn) -> User:
    rows = await query(
        "INSERT INTO users (name, email) VALUES (%s, %s) RETURNING id, name, email",
        data.name,
        data.email,
    )
    return rows[0]


@app.put("/users/{user_id}")
async def update_user(user_id: UserId, data: UserIn) -> User:
    rows = await query(
        "UPDATE users SET name = %s, email = %s WHERE id = %s RETURNING id, name, email",
        data.name,
        data.email,
        user_id,
    )
    return first_or_404(rows)


@app.delete("/users/{user_id}", status_code=204)
async def delete_user(user_id: UserId) -> None:
    first_or_404(await query("DELETE FROM users WHERE id = %s RETURNING id, name, email", user_id))


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=int(os.environ.get("PORT", "8080")))
