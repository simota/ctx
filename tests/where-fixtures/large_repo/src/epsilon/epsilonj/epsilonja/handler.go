package epsilonja

// Handlerepsilonja is a synthetic struct.
type Handlerepsilonja struct {
	ID   int
	Name string
}

// Newepsilonja returns a new handler.
func Newepsilonja() *Handlerepsilonja {
	return &Handlerepsilonja{ID: 1, Name: "epsilonja"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonja) ProcessRequest(req string) string {
	return req
}
