package epsilonce

// Handlerepsilonce is a synthetic struct.
type Handlerepsilonce struct {
	ID   int
	Name string
}

// Newepsilonce returns a new handler.
func Newepsilonce() *Handlerepsilonce {
	return &Handlerepsilonce{ID: 1, Name: "epsilonce"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonce) ProcessRequest(req string) string {
	return req
}
