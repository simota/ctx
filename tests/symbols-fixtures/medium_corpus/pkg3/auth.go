package pkg3

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session3, error) {
	return nil, nil
}

// Session3 represents an authenticated session.
type Session3 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session3, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session3) Render() string {
	return s.Token
}
