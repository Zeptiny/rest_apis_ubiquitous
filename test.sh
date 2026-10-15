#!/usr/bin/env bash
# Runs the same CRUD scenario against every API and checks status codes.
# Usage: ./test.sh            (expects `docker compose up` to be running)
#        HOST=10.0.0.5 ./test.sh
set -uo pipefail

HOST=${HOST:-localhost}
APIS=("python-fastapi:8001" "node-express:8002" "rust-axum:8003" "go-gin:8004")

failures=0
body=""

# expect <status> <method> <path> [json]
expect() {
  local want=$1 method=$2 path=$3 data=${4:-}
  local args=(-s -w $'\n%{http_code}' -X "$method" "$BASE$path")
  [[ -n $data ]] && args+=(-H "Content-Type: application/json" -d "$data")

  local out got
  out=$(curl "${args[@]}") || out=$'\n000'
  got=${out##*$'\n'}
  body=${out%$'\n'*}

  if [[ $got == "$want" ]]; then
    printf "  ok    %-6s %-14s -> %s\n" "$method" "$path" "$got"
  else
    printf "  FAIL  %-6s %-14s -> %s (expected %s) %s\n" "$method" "$path" "$got" "$want" "$body"
    failures=$((failures + 1))
  fi
}

for api in "${APIS[@]}"; do
  name=${api%%:*}
  BASE="http://$HOST:${api##*:}"
  echo "$name ($BASE)"

  # `docker compose up -d` returns before the servers are listening.
  for _ in {1..30}; do
    curl -s -o /dev/null "$BASE/users" && break
    sleep 1
  done

  expect 201 POST /users '{"name":"Ada","email":"ada@example.com"}'
  id=$(grep -o '"id":[0-9]*' <<<"$body" | cut -d: -f2)

  expect 200 GET "/users"
  expect 200 GET "/users/$id"
  expect 200 PUT "/users/$id" '{"name":"Ada Lovelace","email":"ada@example.com"}'
  expect 400 POST /users '{"name":""}'
  expect 400 POST /users 'not json'
  expect 400 GET /users/abc
  expect 204 DELETE "/users/$id"
  expect 404 GET "/users/$id"
  expect 404 DELETE "/users/$id"
  echo
done

# All four APIs share one database: a user created through one is visible through the others.
echo "shared database"
BASE="http://$HOST:8001" expect 201 POST /users '{"name":"Shared","email":"shared@example.com"}'
id=$(grep -o '"id":[0-9]*' <<<"$body" | cut -d: -f2)
BASE="http://$HOST:8002" expect 200 GET "/users/$id"
BASE="http://$HOST:8003" expect 200 PUT "/users/$id" '{"name":"Shared 2","email":"shared@example.com"}'
BASE="http://$HOST:8004" expect 204 DELETE "/users/$id"
BASE="http://$HOST:8001" expect 404 GET "/users/$id"
echo

if ((failures > 0)); then
  echo "$failures check(s) failed"
  exit 1
fi
echo "all checks passed"
