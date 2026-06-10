package api

import "fmt"

type SessionHandler struct{}

func (h *SessionHandler) Handle() error {
	fmt.Println("session handler")
	return nil
}

func RegisterRoutes() {
	_ = &SessionHandler{}
}
