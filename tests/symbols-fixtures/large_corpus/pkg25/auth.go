package pkg25

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session25, error) {
	return nil, nil
}

// Session25 represents an authenticated session.
type Session25 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session25, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session25) Render() string {
	return s.Token
}
