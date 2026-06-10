package pkg16

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session16, error) {
	return nil, nil
}

// Session16 represents an authenticated session.
type Session16 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session16, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session16) Render() string {
	return s.Token
}
