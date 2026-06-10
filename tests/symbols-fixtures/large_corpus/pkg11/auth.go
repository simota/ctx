package pkg11

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session11, error) {
	return nil, nil
}

// Session11 represents an authenticated session.
type Session11 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session11, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session11) Render() string {
	return s.Token
}
