#!/usr/bin/env bash
# Smoke test for the four APIs. Expects `docker compose up` to be running.
#   ./test.sh
#   HOST=10.0.0.5 ./test.sh
set -uo pipefail

HOST=${HOST:-localhost}
APIS=("python-fastapi:8001" "node-express:8002" "rust-axum:8003" "go-gin:8004")

failures=0
status=""
body=""

pass() { printf "  ok    %s\n" "$1"; }
fail() { printf "  FAIL  %s\n" "$1"; failures=$((failures + 1)); }

# Prints a random valid CPF (11 digits), so reruns never collide on the unique constraint.
random_cpf() {
  local d=() i n total
  for i in {0..8}; do d+=($((RANDOM % 10))); done
  for n in 9 10; do
    total=0
    for ((i = 0; i < n; i++)); do total=$((total + d[i] * (n + 1 - i))); done
    d+=($((total * 10 % 11 % 10)))
  done
  printf '%s' "${d[@]}"
}

random_email() { printf '%s-%s%s@example.com' "$1" "$RANDOM" "$RANDOM"; }

# user_json <name> <email> <birthday> <cpf>
user_json() { printf '{"name":"%s","email":"%s","birthday":"%s","cpf":"%s"}' "$@"; }

# json_field <field>: extracts a field from $body (good enough for these flat objects).
json_field() { grep -o "\"$1\":\"\\?[^,\"}]*" <<<"$body" | head -1 | sed 's/^"[^"]*":"\{0,1\}//'; }

# request <base-url> <method> <path> [json] [content-type]: sets $status and $body.
request() {
  local base=$1 method=$2 path=$3 data=${4:-} type=${5:-application/json} out
  local args=(-s -w $'\n%{http_code}' -X "$method" "$base$path")
  [[ -n $data ]] && args+=(-H "Content-Type: $type" --data-raw "$data")
  out=$(curl "${args[@]}") || out=$'\n000'
  status=${out##*$'\n'}
  body=${out%$'\n'*}
}

# expect <status> <description> <method> <path> [json] [content-type]: one request to $BASE.
expect() {
  local want=$1 description=$2
  shift 2
  request "$BASE" "$@"
  local label
  label=$(printf "%-6s %-14s %s  %s" "$1" "$2" "$status" "$description")
  if [[ $status == "$want" ]]; then pass "$label"; else fail "$label (expected $want) $body"; fi
}

# same <description> <method> <path> [json] [content-type]: sends the request to every API.
# All four must answer with the same status code and byte-identical body.
same() {
  local description=$1 first="" out api
  shift
  for api in "${APIS[@]}"; do
    request "http://$HOST:${api##*:}" "$@"
    out="$status $body"
    if [[ $status == 000 ]]; then
      fail "$description: ${api%%:*} is not reachable"
      return
    elif [[ -z $first ]]; then
      first=$out
    elif [[ $out != "$first" ]]; then
      fail "$description: ${APIS[0]%%:*} returned '$first' but ${api%%:*} returned '$out'"
      return
    fi
  done
  pass "$(printf "%-40s %s" "$description" "${first:0:90}")"
}

# 1. The full CRUD scenario, against each API on its own.
for api in "${APIS[@]}"; do
  BASE="http://$HOST:${api##*:}"
  echo "${api%%:*} ($BASE)"

  # `docker compose up -d` returns before the servers are listening.
  for _ in {1..30}; do
    curl -s -o /dev/null "$BASE/users" && break
    sleep 1
  done

  cpf=$(random_cpf)
  email=$(random_email ada)
  expect 201 "create" POST /users "$(user_json Ada "$email" 1990-05-17 "$cpf")"
  id=$(json_field id)
  expect 200 "list" GET /users
  expect 200 "read" GET "/users/$id"
  expect 200 "update" PUT "/users/$id" "$(user_json "Ada Lovelace" "$email" 1990-05-17 "$cpf")"
  expect 400 "empty name" POST /users "$(user_json "" "$(random_email x)" 1990-05-17 "$(random_cpf)")"
  expect 400 "bad email" POST /users "$(user_json Ada not-an-email 1990-05-17 "$(random_cpf)")"
  expect 400 "impossible birthday" POST /users "$(user_json Ada "$(random_email x)" 1990-02-30 "$(random_cpf)")"
  expect 400 "bad cpf check digit" POST /users "$(user_json Ada "$(random_email x)" 1990-05-17 52998224724)"
  expect 409 "duplicate cpf" POST /users "$(user_json Ada "$(random_email x)" 1990-05-17 "$cpf")"
  expect 409 "duplicate email" POST /users "$(user_json Ada "$email" 1990-05-17 "$(random_cpf)")"
  expect 400 "invalid id" GET /users/abc
  expect 204 "delete" DELETE "/users/$id"
  expect 404 "read deleted" GET "/users/$id"
  expect 404 "delete deleted" DELETE "/users/$id"
  echo
done

# 2. All four APIs share one database and must give identical answers to identical requests.
echo "identical responses (shared database)"
BASE="http://$HOST:8001"
cpf=$(random_cpf)
masked="${cpf:0:3}.${cpf:3:3}.${cpf:6:3}-${cpf:9:2}"
email=$(random_email grace)
expect 201 "create with masked cpf $masked" POST /users "$(user_json "Grace Hopper" "$email" 1906-12-09 "$masked")"
id=$(json_field id)
if [[ $(json_field cpf) == "$cpf" ]]; then pass "cpf is stored as 11 digits"; else fail "cpf returned as '$(json_field cpf)', expected $cpf"; fi

same "read the same user" GET "/users/$id"
same "list" GET /users
same "body is not JSON" POST /users 'not json'
same "body is an array" POST /users '[]'
same "content type is not JSON" POST /users "$(user_json A "$(random_email x)" 1990-05-17 "$(random_cpf)")" text/plain
same "missing name" POST /users '{}'
same "name is not a string" POST /users '{"name":5,"email":"a@b","birthday":"1990-05-17","cpf":"52998224725"}'
same "email without @" POST /users "$(user_json A nope 1990-05-17 "$(random_cpf)")"
same "email with two @" POST /users "$(user_json A a@b@c 1990-05-17 "$(random_cpf)")"
same "birthday not zero-padded" POST /users "$(user_json A "$(random_email x)" 1990-5-17 "$(random_cpf)")"
same "birthday in the future" POST /users "$(user_json A "$(random_email x)" 2999-01-01 "$(random_cpf)")"
same "birthday before 1900" POST /users "$(user_json A "$(random_email x)" 1899-12-31 "$(random_cpf)")"
same "cpf with all digits equal" POST /users "$(user_json A "$(random_email x)" 1990-05-17 111.111.111-11)"
same "cpf with a partial mask" POST /users "$(user_json A "$(random_email x)" 1990-05-17 529.982.24725)"
same "duplicate cpf" POST /users "$(user_json A "$(random_email x)" 1990-05-17 "$cpf")"
same "duplicate email" POST /users "$(user_json A "$email" 1990-05-17 "$(random_cpf)")"
same "update with an invalid cpf" PUT "/users/$id" "$(user_json A "$email" 1990-05-17 123)"
same "update a missing user" PUT /users/999999999 "$(user_json A "$(random_email x)" 1990-05-17 "$(random_cpf)")"
same "update with an invalid id" PUT /users/abc '{}'
same "id is zero" GET /users/0
same "id is negative" GET /users/-1
same "id beyond BIGINT" GET /users/99999999999999999999
same "id is BIGINT max" GET /users/9223372036854775807
same "delete a missing user" DELETE /users/999999999

BASE="http://$HOST:8004"
expect 200 "update through another API" PUT "/users/$id" "$(user_json "Grace B. Hopper" "$email" 1906-12-09 "$cpf")"
if [[ $(json_field updated_at) > $(json_field inserted_at) ]]; then
  pass "update refreshed updated_at"
else
  fail "updated_at ($(json_field updated_at)) not after inserted_at ($(json_field inserted_at))"
fi
same "read the updated user" GET "/users/$id"

BASE="http://$HOST:8003"
expect 204 "delete through another API" DELETE "/users/$id"
same "read the deleted user" GET "/users/$id"
echo

if ((failures > 0)); then
  echo "$failures check(s) failed"
  exit 1
fi
echo "all checks passed"
