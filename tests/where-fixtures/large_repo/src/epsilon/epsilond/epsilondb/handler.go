package epsilondb

// Handlerepsilondb is a synthetic struct.
type Handlerepsilondb struct {
	ID   int
	Name string
}

// Newepsilondb returns a new handler.
func Newepsilondb() *Handlerepsilondb {
	return &Handlerepsilondb{ID: 1, Name: "epsilondb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerepsilondb) ProcessRequest(req string) string {
	return req
}
