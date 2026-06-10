package pkg2

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session2, error) {
	return nil, nil
}

// Session2 represents an authenticated session.
type Session2 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session2, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session2) Render() string {
	return s.Token
}
