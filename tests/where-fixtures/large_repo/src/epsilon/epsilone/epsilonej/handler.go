package epsilonej

// Handlerepsilonej is a synthetic struct.
type Handlerepsilonej struct {
	ID   int
	Name string
}

// Newepsilonej returns a new handler.
func Newepsilonej() *Handlerepsilonej {
	return &Handlerepsilonej{ID: 1, Name: "epsilonej"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonej) ProcessRequest(req string) string {
	return req
}
