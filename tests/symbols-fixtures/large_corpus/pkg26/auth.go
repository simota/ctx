package pkg26

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session26, error) {
	return nil, nil
}

// Session26 represents an authenticated session.
type Session26 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session26, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session26) Render() string {
	return s.Token
}
