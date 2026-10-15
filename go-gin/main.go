package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"os"
	"strconv"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

type User struct {
	ID    int64  `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

type userInput struct {
	Name  string `json:"name" binding:"required"`
	Email string `json:"email" binding:"required"`
}

type server struct {
	db *pgxpool.Pool
}

// parseID writes a 400 response and returns false when :id is not a positive integer.
func parseID(c *gin.Context) (int64, bool) {
	id, err := strconv.ParseInt(c.Param("id"), 10, 64)
	if err != nil || id < 1 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid id"})
		return 0, false
	}
	return id, true
}

// bindInput writes a 400 response and returns false when name or email is missing.
func bindInput(c *gin.Context) (userInput, bool) {
	var input userInput
	if err := c.ShouldBindJSON(&input); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "name and email are required"})
		return input, false
	}
	return input, true
}

func notFound(c *gin.Context) {
	c.JSON(http.StatusNotFound, gin.H{"error": "user not found"})
}

func internalError(c *gin.Context, err error) {
	log.Printf("database error: %v", err)
	c.JSON(http.StatusInternalServerError, gin.H{"error": "internal server error"})
}

// respondWithUser runs a query that returns at most one user and writes it as the response.
func (s *server) respondWithUser(c *gin.Context, status int, sql string, args ...any) {
	var user User
	err := s.db.QueryRow(c.Request.Context(), sql, args...).Scan(&user.ID, &user.Name, &user.Email)
	switch {
	case errors.Is(err, pgx.ErrNoRows):
		notFound(c)
	case err != nil:
		internalError(c, err)
	default:
		c.JSON(status, user)
	}
}

func (s *server) list(c *gin.Context) {
	rows, _ := s.db.Query(c.Request.Context(), "SELECT id, name, email FROM users ORDER BY id")
	users, err := pgx.CollectRows(rows, pgx.RowToStructByPos[User])
	if err != nil {
		internalError(c, err)
		return
	}
	c.JSON(http.StatusOK, users)
}

func (s *server) get(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}
	s.respondWithUser(c, http.StatusOK, "SELECT id, name, email FROM users WHERE id = $1", id)
}

func (s *server) create(c *gin.Context) {
	input, ok := bindInput(c)
	if !ok {
		return
	}
	s.respondWithUser(c, http.StatusCreated,
		"INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email",
		input.Name, input.Email)
}

func (s *server) update(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}
	input, ok := bindInput(c)
	if !ok {
		return
	}
	s.respondWithUser(c, http.StatusOK,
		"UPDATE users SET name = $1, email = $2 WHERE id = $3 RETURNING id, name, email",
		input.Name, input.Email, id)
}

func (s *server) delete(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}
	tag, err := s.db.Exec(c.Request.Context(), "DELETE FROM users WHERE id = $1", id)
	switch {
	case err != nil:
		internalError(c, err)
	case tag.RowsAffected() == 0:
		notFound(c)
	default:
		c.Status(http.StatusNoContent)
	}
}

func getenv(key, fallback string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return fallback
}

func main() {
	db, err := pgxpool.New(context.Background(),
		getenv("DATABASE_URL", "postgres://api:api@localhost:5432/users"))
	if err != nil {
		log.Fatal(err)
	}
	defer db.Close()
	s := &server{db: db}

	r := gin.Default()
	r.GET("/users", s.list)
	r.GET("/users/:id", s.get)
	r.POST("/users", s.create)
	r.PUT("/users/:id", s.update)
	r.DELETE("/users/:id", s.delete)

	if err := r.Run(":" + getenv("PORT", "8080")); err != nil {
		log.Fatal(err)
	}
}
