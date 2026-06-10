package epsilonca

// Handlerepsilonca is a synthetic struct.
type Handlerepsilonca struct {
	ID   int
	Name string
}

// Newepsilonca returns a new handler.
func Newepsilonca() *Handlerepsilonca {
	return &Handlerepsilonca{ID: 1, Name: "epsilonca"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonca) ProcessRequest(req string) string {
	return req
}
