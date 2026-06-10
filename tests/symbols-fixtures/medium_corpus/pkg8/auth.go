package pkg8

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session8, error) {
	return nil, nil
}

// Session8 represents an authenticated session.
type Session8 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session8, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session8) Render() string {
	return s.Token
}
