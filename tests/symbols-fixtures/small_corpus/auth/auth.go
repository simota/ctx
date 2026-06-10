package auth

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session, error) {
	return nil, nil
}

// Session represents an authenticated session.
type Session struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session) Render() string {
	return s.Token
}
