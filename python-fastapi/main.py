import itertools
import os
from typing import Annotated

import uvicorn
from fastapi import FastAPI, HTTPException, Path, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from starlette.exceptions import HTTPException as StarletteHTTPException

app = FastAPI(title="Users API (FastAPI)")


class UserIn(BaseModel):
    name: str = Field(min_length=1)
    email: str = Field(min_length=1)


class User(BaseModel):
    id: int
    name: str
    email: str


UserId = Annotated[int, Path(gt=0)]

users: dict[int, User] = {}
ids = itertools.count(1)


@app.exception_handler(StarletteHTTPException)
async def http_error(request: Request, exc: StarletteHTTPException) -> JSONResponse:
    return JSONResponse(status_code=exc.status_code, content={"error": exc.detail})


@app.exception_handler(RequestValidationError)
async def validation_error(request: Request, exc: RequestValidationError) -> JSONResponse:
    bad_path = any(err["loc"][0] == "path" for err in exc.errors())
    message = "invalid id" if bad_path else "name and email are required"
    return JSONResponse(status_code=400, content={"error": message})


def find_user(user_id: int) -> User:
    if user_id not in users:
        raise HTTPException(status_code=404, detail="user not found")
    return users[user_id]


@app.get("/users")
async def list_users() -> list[User]:
    return list(users.values())


@app.get("/users/{user_id}")
async def get_user(user_id: UserId) -> User:
    return find_user(user_id)


@app.post("/users", status_code=201)
async def create_user(data: UserIn) -> User:
    user = User(id=next(ids), **data.model_dump())
    users[user.id] = user
    return user


@app.put("/users/{user_id}")
async def update_user(user_id: UserId, data: UserIn) -> User:
    find_user(user_id)
    users[user_id] = User(id=user_id, **data.model_dump())
    return users[user_id]


@app.delete("/users/{user_id}", status_code=204)
async def delete_user(user_id: UserId) -> None:
    find_user(user_id)
    del users[user_id]


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=int(os.environ.get("PORT", "8080")))
