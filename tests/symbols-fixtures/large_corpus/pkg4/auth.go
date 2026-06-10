package pkg4

import "context"

// LoginUser authenticates a user.
//
// Returns a session token on success.
func LoginUser(ctx context.Context, email, password string) (*Session4, error) {
	return nil, nil
}

// Session4 represents an authenticated session.
type Session4 struct {
	Token string
	User  string
}

// BuildIndex constructs the user index.
func BuildIndex(root string) (*Session4, error) {
	return nil, nil
}

func internalHelper() {}

// Render formats the session for display.
func (s *Session4) Render() string {
	return s.Token
}
