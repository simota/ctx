package epsilonba

// Handlerepsilonba is a synthetic struct.
type Handlerepsilonba struct {
	ID   int
	Name string
}

// Newepsilonba returns a new handler.
func Newepsilonba() *Handlerepsilonba {
	return &Handlerepsilonba{ID: 1, Name: "epsilonba"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilonba) ProcessRequest(req string) string {
	return req
}
