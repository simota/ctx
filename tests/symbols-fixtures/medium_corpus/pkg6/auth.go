package pkg6

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session6, error) {
	return nil, nil
}

// Session6 represents an authenticated session.
type Session6 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session6, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session6) Render() string {
	return s.Token
}
