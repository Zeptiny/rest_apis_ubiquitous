package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"os"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Dates and timestamps are formatted by Postgres so all four APIs return identical strings.
const columns = `id, name, email, to_char(birthday, 'YYYY-MM-DD') AS birthday, cpf,
	to_char(inserted_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS inserted_at,
	to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS updated_at`

const (
	invalidBody     = "invalid request body"
	invalidName     = "name must be a non-empty string"
	invalidEmail    = "email must be a valid email address"
	invalidBirthday = "birthday must be a date (YYYY-MM-DD) between 1900-01-01 and today"
	invalidCPF      = "cpf must be a valid CPF"
)

var (
	cpfFormat        = regexp.MustCompile(`^([0-9]{11}|[0-9]{3}\.[0-9]{3}\.[0-9]{3}-[0-9]{2})$`)
	earliestBirthday = time.Date(1900, 1, 1, 0, 0, 0, 0, time.UTC)
)

type User struct {
	ID         int64  `json:"id"`
	Name       string `json:"name"`
	Email      string `json:"email"`
	Birthday   string `json:"birthday"`
	CPF        string `json:"cpf"`
	InsertedAt string `json:"inserted_at"`
	UpdatedAt  string `json:"updated_at"`
}

type userInput struct {
	Name     string
	Email    string
	Birthday time.Time
	CPF      string
}

type server struct {
	db *pgxpool.Pool
}

func isEmail(value string) bool {
	return strings.Count(value, "@") == 1 &&
		!strings.HasPrefix(value, "@") && !strings.HasSuffix(value, "@")
}

// parseBirthday rejects malformed or impossible dates (e.g. 2024-02-30) and dates out of range.
func parseBirthday(value string) (time.Time, bool) {
	date, err := time.Parse(time.DateOnly, value)
	if err != nil || date.Before(earliestBirthday) || date.After(time.Now()) {
		return date, false
	}
	return date, true
}

// normalizeCPF accepts 11 digits or the 000.000.000-00 mask and checks both verification digits.
func normalizeCPF(value string) (string, bool) {
	if !cpfFormat.MatchString(value) {
		return "", false
	}
	digits := strings.NewReplacer(".", "", "-", "").Replace(value)
	if strings.Count(digits, digits[:1]) == len(digits) {
		return "", false
	}
	for n := 9; n <= 10; n++ {
		total := 0
		for i := range n {
			total += int(digits[i]-'0') * (n + 1 - i)
		}
		if total*10%11%10 != int(digits[n]-'0') {
			return "", false
		}
	}
	return digits, true
}

// validateInput checks the fields in the same order as the other APIs.
// It returns the first problem found as an error message.
func validateInput(body map[string]any) (userInput, string) {
	var input userInput
	var ok bool
	if input.Name, _ = body["name"].(string); input.Name == "" {
		return input, invalidName
	}
	if input.Email, _ = body["email"].(string); !isEmail(input.Email) {
		return input, invalidEmail
	}
	birthday, _ := body["birthday"].(string)
	if input.Birthday, ok = parseBirthday(birthday); !ok {
		return input, invalidBirthday
	}
	cpf, _ := body["cpf"].(string)
	if input.CPF, ok = normalizeCPF(cpf); !ok {
		return input, invalidCPF
	}
	return input, ""
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

// bindInput writes a 400 response and returns false when the body is not a valid user.
func bindInput(c *gin.Context) (userInput, bool) {
	var body map[string]any
	if !strings.EqualFold(c.ContentType(), "application/json") || c.ShouldBindJSON(&body) != nil || body == nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": invalidBody})
		return userInput{}, false
	}
	input, problem := validateInput(body)
	if problem != "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": problem})
		return input, false
	}
	return input, true
}

func notFound(c *gin.Context) {
	c.JSON(http.StatusNotFound, gin.H{"error": "user not found"})
}

// databaseError maps duplicate email/cpf to 409 and anything else to 500.
func databaseError(c *gin.Context, err error) {
	var pgErr *pgconn.PgError
	if errors.As(err, &pgErr) && pgErr.Code == "23505" {
		field := "email"
		if pgErr.ConstraintName == "users_cpf_key" {
			field = "cpf"
		}
		c.JSON(http.StatusConflict, gin.H{"error": field + " already registered"})
		return
	}
	log.Printf("database error: %v", err)
	c.JSON(http.StatusInternalServerError, gin.H{"error": "internal server error"})
}

// respondWithUser runs a query that returns at most one user and writes it as the response.
func (s *server) respondWithUser(c *gin.Context, status int, sql string, args ...any) {
	rows, _ := s.db.Query(c.Request.Context(), sql, args...)
	user, err := pgx.CollectOneRow(rows, pgx.RowToStructByPos[User])
	switch {
	case errors.Is(err, pgx.ErrNoRows):
		notFound(c)
	case err != nil:
		databaseError(c, err)
	default:
		c.JSON(status, user)
	}
}

func (s *server) list(c *gin.Context) {
	rows, _ := s.db.Query(c.Request.Context(), "SELECT "+columns+" FROM users ORDER BY id")
	users, err := pgx.CollectRows(rows, pgx.RowToStructByPos[User])
	if err != nil {
		databaseError(c, err)
		return
	}
	c.JSON(http.StatusOK, users)
}

func (s *server) get(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}
	s.respondWithUser(c, http.StatusOK, "SELECT "+columns+" FROM users WHERE id = $1", id)
}

func (s *server) create(c *gin.Context) {
	input, ok := bindInput(c)
	if !ok {
		return
	}
	s.respondWithUser(c, http.StatusCreated,
		"INSERT INTO users (name, email, birthday, cpf) VALUES ($1, $2, $3, $4) RETURNING "+columns,
		input.Name, input.Email, input.Birthday, input.CPF)
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
		`UPDATE users SET name = $1, email = $2, birthday = $3, cpf = $4, updated_at = now()
		WHERE id = $5 RETURNING `+columns,
		input.Name, input.Email, input.Birthday, input.CPF, id)
}

func (s *server) delete(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}
	tag, err := s.db.Exec(c.Request.Context(), "DELETE FROM users WHERE id = $1", id)
	switch {
	case err != nil:
		databaseError(c, err)
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
