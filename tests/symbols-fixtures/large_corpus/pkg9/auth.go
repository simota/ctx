package pkg9

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session9, error) {
	return nil, nil
}

// Session9 represents an authenticated session.
type Session9 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session9, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session9) Render() string {
	return s.Token
}
