package pkg10

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session10, error) {
	return nil, nil
}

// Session10 represents an authenticated session.
type Session10 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session10, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session10) Render() string {
	return s.Token
}
