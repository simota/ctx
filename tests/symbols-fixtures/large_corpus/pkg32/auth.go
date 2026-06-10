package pkg32

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session32, error) {
	return nil, nil
}

// Session32 represents an authenticated session.
type Session32 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session32, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session32) Render() string {
	return s.Token
}
