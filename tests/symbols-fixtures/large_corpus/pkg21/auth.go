package pkg21

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session21, error) {
	return nil, nil
}

// Session21 represents an authenticated session.
type Session21 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session21, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session21) Render() string {
	return s.Token
}
