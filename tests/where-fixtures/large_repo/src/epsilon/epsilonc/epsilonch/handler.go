package epsilonch

// Handlerepsilonch is a synthetic struct.
type Handlerepsilonch struct {
	ID   int
	Name string
}

// Newepsilonch returns a new handler.
func Newepsilonch() *Handlerepsilonch {
	return &Handlerepsilonch{ID: 1, Name: "epsilonch"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonch) ProcessRequest(req string) string {
	return req
}
