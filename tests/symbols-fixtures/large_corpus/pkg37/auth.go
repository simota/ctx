package pkg37

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session37, error) {
	return nil, nil
}

// Session37 represents an authenticated session.
type Session37 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session37, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session37) Render() string {
	return s.Token
}
