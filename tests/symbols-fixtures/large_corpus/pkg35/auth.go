package pkg35

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session35, error) {
	return nil, nil
}

// Session35 represents an authenticated session.
type Session35 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session35, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session35) Render() string {
	return s.Token
}
