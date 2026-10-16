const express = require("express");
const { Pool, types } = require("pg");

// BIGINT ids arrive as strings by default; they fit safely in a JS number here.
types.setTypeParser(types.builtins.INT8, Number);

const pool = new Pool({
  connectionString: process.env.DATABASE_URL ?? "postgres://api:api@localhost:5432/users",
});
// Without a listener, an idle connection dropped by Postgres crashes the process.
pool.on("error", (err) => console.error("idle database connection failed:", err.message));

// Dates and timestamps are formatted by Postgres so all four APIs return identical strings.
const COLUMNS = `id, name, email, to_char(birthday, 'YYYY-MM-DD') AS birthday, cpf,
  to_char(inserted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS inserted_at,
  to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at`;

const MAX_ID = 2n ** 63n - 1n; // Postgres BIGINT

const INVALID_BODY = "invalid request body";
const INVALID_FIELD = {
  name: "name must be a non-empty string",
  email: "email must be a valid email address",
  birthday: "birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today",
  cpf: "cpf must be a valid CPF",
};

function isEmail(value) {
  return (
    typeof value === "string" &&
    value.split("@").length === 2 &&
    !value.startsWith("@") &&
    !value.endsWith("@")
  );
}

function isBirthday(value) {
  if (typeof value !== "string" || !/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/.test(value)) return false;
  // Rejects dates like 2024-02-30, which do not survive the round trip.
  const date = new Date(`${value}T00:00:00Z`);
  if (Number.isNaN(date.getTime()) || date.toISOString().slice(0, 10) !== value) return false;
  const today = new Date().toISOString().slice(0, 10);
  return value >= "1900-01-01" && value <= today;
}

// Accepts 11 digits or the 000.000.000-00 mask, checks both verification digits.
// Returns the 11 digits, or null when the CPF is invalid.
function normalizeCpf(value) {
  if (typeof value !== "string" || !/^([0-9]{11}|[0-9]{3}\.[0-9]{3}\.[0-9]{3}-[0-9]{2})$/.test(value)) {
    return null;
  }
  const digits = value.replace(/[.-]/g, "");
  if (/^(.)\1*$/.test(digits)) return null;
  for (const n of [9, 10]) {
    let total = 0;
    for (let i = 0; i < n; i++) total += Number(digits[i]) * (n + 1 - i);
    if (((total * 10) % 11) % 10 !== Number(digits[n])) return null;
  }
  return digits;
}

// Returns { input } with normalised fields, or { error } with the first problem found.
// express.json() leaves the body undefined unless the request is application/json.
function parseInput(body) {
  if (body === null || typeof body !== "object" || Array.isArray(body)) {
    return { error: INVALID_BODY };
  }
  const { name, email, birthday } = body;
  if (typeof name !== "string" || name === "") return { error: INVALID_FIELD.name };
  if (!isEmail(email)) return { error: INVALID_FIELD.email };
  if (!isBirthday(birthday)) return { error: INVALID_FIELD.birthday };
  const cpf = normalizeCpf(body.cpf);
  if (!cpf) return { error: INVALID_FIELD.cpf };
  return { input: { name, email, birthday, cpf } };
}

function notFound(res) {
  res.status(404).json({ error: "user not found" });
}

const app = express();
app.use(express.json());

// Every route with an :id must receive a positive integer that fits in a BIGINT.
app.param("id", (req, res, next, raw) => {
  const id = /^\d+$/.test(raw) ? BigInt(raw) : 0n;
  if (id < 1n || id > MAX_ID) {
    return res.status(400).json({ error: "invalid id" });
  }
  req.userId = id;
  next();
});

app.get("/users", async (req, res) => {
  const { rows } = await pool.query(`SELECT ${COLUMNS} FROM users ORDER BY id`);
  res.json(rows);
});

app.get("/users/:id", async (req, res) => {
  const { rows } = await pool.query(`SELECT ${COLUMNS} FROM users WHERE id = $1`, [req.userId]);
  if (rows.length === 0) return notFound(res);
  res.json(rows[0]);
});

app.post("/users", async (req, res) => {
  const { input, error } = parseInput(req.body);
  if (error) return res.status(400).json({ error });
  const { rows } = await pool.query(
    `INSERT INTO users (name, email, birthday, cpf) VALUES ($1, $2, $3, $4) RETURNING ${COLUMNS}`,
    [input.name, input.email, input.birthday, input.cpf],
  );
  res.status(201).json(rows[0]);
});

app.put("/users/:id", async (req, res) => {
  const { input, error } = parseInput(req.body);
  if (error) return res.status(400).json({ error });
  const { rows } = await pool.query(
    `UPDATE users SET name = $1, email = $2, birthday = $3, cpf = $4, updated_at = now()
     WHERE id = $5 RETURNING ${COLUMNS}`,
    [input.name, input.email, input.birthday, input.cpf, req.userId],
  );
  if (rows.length === 0) return notFound(res);
  res.json(rows[0]);
});

app.delete("/users/:id", async (req, res) => {
  const { rowCount } = await pool.query("DELETE FROM users WHERE id = $1", [req.userId]);
  if (rowCount === 0) return notFound(res);
  res.status(204).end();
});

// Malformed JSON, duplicate email/cpf, and unexpected failures.
app.use((err, req, res, next) => {
  if (err.type === "entity.parse.failed") {
    return res.status(400).json({ error: INVALID_BODY });
  }
  if (err.code === "23505") {
    const field = err.constraint === "users_cpf_key" ? "cpf" : "email";
    return res.status(409).json({ error: `${field} already registered` });
  }
  console.error(err);
  res.status(500).json({ error: "internal server error" });
});

const port = process.env.PORT || 8080;
app.listen(port, () => console.log(`listening on port ${port}`));
