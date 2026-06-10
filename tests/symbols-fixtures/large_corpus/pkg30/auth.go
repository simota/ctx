package pkg30

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session30, error) {
	return nil, nil
}

// Session30 represents an authenticated session.
type Session30 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session30, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session30) Render() string {
	return s.Token
}
