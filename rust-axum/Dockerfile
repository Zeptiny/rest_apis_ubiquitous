FROM rust:1-slim-bookworm AS build

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=build /src/target/release/users-api /users-api

EXPOSE 8080
ENTRYPOINT ["/users-api"]
