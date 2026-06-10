package pkg7

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session7, error) {
	return nil, nil
}

// Session7 represents an authenticated session.
type Session7 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session7, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session7) Render() string {
	return s.Token
}
