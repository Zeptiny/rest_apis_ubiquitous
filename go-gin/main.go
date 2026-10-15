package main

import (
	"cmp"
	"net/http"
	"os"
	"slices"
	"strconv"
	"sync"

	"github.com/gin-gonic/gin"
)

type User struct {
	ID    int    `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

type userInput struct {
	Name  string `json:"name" binding:"required"`
	Email string `json:"email" binding:"required"`
}

type store struct {
	mu     sync.RWMutex
	users  map[int]User
	nextID int
}

// parseID writes a 400 response and returns false when :id is not a positive integer.
func parseID(c *gin.Context) (int, bool) {
	id, err := strconv.Atoi(c.Param("id"))
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

func (s *store) list(c *gin.Context) {
	s.mu.RLock()
	users := make([]User, 0, len(s.users))
	for _, user := range s.users {
		users = append(users, user)
	}
	s.mu.RUnlock()

	slices.SortFunc(users, func(a, b User) int { return cmp.Compare(a.ID, b.ID) })
	c.JSON(http.StatusOK, users)
}

func (s *store) get(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}

	s.mu.RLock()
	user, found := s.users[id]
	s.mu.RUnlock()

	if !found {
		notFound(c)
		return
	}
	c.JSON(http.StatusOK, user)
}

func (s *store) create(c *gin.Context) {
	input, ok := bindInput(c)
	if !ok {
		return
	}

	s.mu.Lock()
	user := User{ID: s.nextID, Name: input.Name, Email: input.Email}
	s.users[user.ID] = user
	s.nextID++
	s.mu.Unlock()

	c.JSON(http.StatusCreated, user)
}

func (s *store) update(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}
	input, ok := bindInput(c)
	if !ok {
		return
	}

	s.mu.Lock()
	_, found := s.users[id]
	user := User{ID: id, Name: input.Name, Email: input.Email}
	if found {
		s.users[id] = user
	}
	s.mu.Unlock()

	if !found {
		notFound(c)
		return
	}
	c.JSON(http.StatusOK, user)
}

func (s *store) delete(c *gin.Context) {
	id, ok := parseID(c)
	if !ok {
		return
	}

	s.mu.Lock()
	_, found := s.users[id]
	delete(s.users, id)
	s.mu.Unlock()

	if !found {
		notFound(c)
		return
	}
	c.Status(http.StatusNoContent)
}

func main() {
	s := &store{users: map[int]User{}, nextID: 1}

	r := gin.Default()
	r.GET("/users", s.list)
	r.GET("/users/:id", s.get)
	r.POST("/users", s.create)
	r.PUT("/users/:id", s.update)
	r.DELETE("/users/:id", s.delete)

	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}
	if err := r.Run(":" + port); err != nil {
		panic(err)
	}
}
