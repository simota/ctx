package pkg18

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session18, error) {
	return nil, nil
}

// Session18 represents an authenticated session.
type Session18 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session18, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session18) Render() string {
	return s.Token
}
