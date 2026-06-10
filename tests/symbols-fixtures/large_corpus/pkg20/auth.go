package pkg20

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session20, error) {
	return nil, nil
}

// Session20 represents an authenticated session.
type Session20 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session20, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session20) Render() string {
	return s.Token
}
