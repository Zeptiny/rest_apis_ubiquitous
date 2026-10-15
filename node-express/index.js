const express = require("express");
const { Pool, types } = require("pg");

// BIGINT ids arrive as strings by default; they fit safely in a JS number here.
types.setTypeParser(types.builtins.INT8, Number);

const pool = new Pool({
  connectionString: process.env.DATABASE_URL ?? "postgres://api:api@localhost:5432/users",
});
// Without a listener, an idle connection dropped by Postgres crashes the process.
pool.on("error", (err) => console.error("idle database connection failed:", err.message));

const MAX_ID = 2n ** 63n - 1n; // Postgres BIGINT

const app = express();
app.use(express.json());

// Returns { name, email } when both are non-empty strings, otherwise null.
function parseInput(body) {
  const { name, email } = body ?? {};
  if (typeof name !== "string" || typeof email !== "string" || !name || !email) {
    return null;
  }
  return { name, email };
}

function notFound(res) {
  res.status(404).json({ error: "user not found" });
}

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
  const { rows } = await pool.query("SELECT id, name, email FROM users ORDER BY id");
  res.json(rows);
});

app.get("/users/:id", async (req, res) => {
  const { rows } = await pool.query("SELECT id, name, email FROM users WHERE id = $1", [
    req.userId,
  ]);
  if (rows.length === 0) return notFound(res);
  res.json(rows[0]);
});

app.post("/users", async (req, res) => {
  const input = parseInput(req.body);
  if (!input) return res.status(400).json({ error: "name and email are required" });
  const { rows } = await pool.query(
    "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email",
    [input.name, input.email],
  );
  res.status(201).json(rows[0]);
});

app.put("/users/:id", async (req, res) => {
  const input = parseInput(req.body);
  if (!input) return res.status(400).json({ error: "name and email are required" });
  const { rows } = await pool.query(
    "UPDATE users SET name = $1, email = $2 WHERE id = $3 RETURNING id, name, email",
    [input.name, input.email, req.userId],
  );
  if (rows.length === 0) return notFound(res);
  res.json(rows[0]);
});

app.delete("/users/:id", async (req, res) => {
  const { rowCount } = await pool.query("DELETE FROM users WHERE id = $1", [req.userId]);
  if (rowCount === 0) return notFound(res);
  res.status(204).end();
});

// Malformed JSON is rejected by express.json(); anything else is an unexpected failure.
app.use((err, req, res, next) => {
  if (err.type === "entity.parse.failed") {
    return res.status(400).json({ error: "name and email are required" });
  }
  console.error(err);
  res.status(500).json({ error: "internal server error" });
});

const port = process.env.PORT || 8080;
app.listen(port, () => console.log(`listening on port ${port}`));
