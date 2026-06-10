package pkg14

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session14, error) {
	return nil, nil
}

// Session14 represents an authenticated session.
type Session14 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session14, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session14) Render() string {
	return s.Token
}
