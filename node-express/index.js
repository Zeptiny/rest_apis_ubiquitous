const express = require("express");

const app = express();
app.use(express.json());

const users = new Map();
let nextId = 1;

// Returns { name, email } when both are non-empty strings, otherwise null.
function parseInput(body) {
  const { name, email } = body ?? {};
  if (typeof name !== "string" || typeof email !== "string" || !name || !email) {
    return null;
  }
  return { name, email };
}

// Every route with an :id must receive a positive integer.
app.param("id", (req, res, next, raw) => {
  const id = Number(raw);
  if (!Number.isInteger(id) || id < 1) {
    return res.status(400).json({ error: "invalid id" });
  }
  req.userId = id;
  next();
});

app.get("/users", (req, res) => {
  res.json([...users.values()]);
});

app.get("/users/:id", (req, res) => {
  const user = users.get(req.userId);
  if (!user) return res.status(404).json({ error: "user not found" });
  res.json(user);
});

app.post("/users", (req, res) => {
  const input = parseInput(req.body);
  if (!input) return res.status(400).json({ error: "name and email are required" });
  const user = { id: nextId++, ...input };
  users.set(user.id, user);
  res.status(201).json(user);
});

app.put("/users/:id", (req, res) => {
  const input = parseInput(req.body);
  if (!input) return res.status(400).json({ error: "name and email are required" });
  if (!users.has(req.userId)) return res.status(404).json({ error: "user not found" });
  const user = { id: req.userId, ...input };
  users.set(user.id, user);
  res.json(user);
});

app.delete("/users/:id", (req, res) => {
  if (!users.delete(req.userId)) return res.status(404).json({ error: "user not found" });
  res.status(204).end();
});

// express.json() rejects malformed bodies before they reach a route.
app.use((err, req, res, next) => {
  if (err.type === "entity.parse.failed") {
    return res.status(400).json({ error: "name and email are required" });
  }
  next(err);
});

const port = process.env.PORT || 8080;
app.listen(port, () => console.log(`listening on port ${port}`));
