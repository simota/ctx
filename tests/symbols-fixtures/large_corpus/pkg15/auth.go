package pkg15

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session15, error) {
	return nil, nil
}

// Session15 represents an authenticated session.
type Session15 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session15, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session15) Render() string {
	return s.Token
}
