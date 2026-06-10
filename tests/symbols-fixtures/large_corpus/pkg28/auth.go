package pkg28

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session28, error) {
	return nil, nil
}

// Session28 represents an authenticated session.
type Session28 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session28, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session28) Render() string {
	return s.Token
}
