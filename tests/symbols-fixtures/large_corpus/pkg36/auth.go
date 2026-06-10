package pkg36

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session36, error) {
	return nil, nil
}

// Session36 represents an authenticated session.
type Session36 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session36, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session36) Render() string {
	return s.Token
}
