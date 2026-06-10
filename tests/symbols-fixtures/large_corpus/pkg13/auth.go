package pkg13

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session13, error) {
	return nil, nil
}

// Session13 represents an authenticated session.
type Session13 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session13, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session13) Render() string {
	return s.Token
}
