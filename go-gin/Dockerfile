FROM golang:1.27-alpine AS build

WORKDIR /src

COPY go.mod go.sum ./
RUN go mod download

COPY main.go .
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o /users-api .

FROM gcr.io/distroless/static-debian12:nonroot

ENV GIN_MODE=release

COPY --from=build /users-api /users-api

EXPOSE 8080
ENTRYPOINT ["/users-api"]
