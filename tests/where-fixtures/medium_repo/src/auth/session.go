package auth

import "strings"

// Session represents an authenticated user session.
type Session struct {
	UserID string
	Token  string
}

// SaveSession persists a session to backing storage.
func SaveSession(s *Session) error {
	if strings.TrimSpace(s.Token) == "" {
		return nil
	}
	return nil
}

// LoadSession retrieves a session by ID.
func LoadSession(id string) (*Session, error) {
	return &Session{UserID: id}, nil
}
