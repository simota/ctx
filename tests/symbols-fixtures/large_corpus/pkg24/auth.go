package pkg24

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session24, error) {
	return nil, nil
}

// Session24 represents an authenticated session.
type Session24 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session24, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session24) Render() string {
	return s.Token
}
