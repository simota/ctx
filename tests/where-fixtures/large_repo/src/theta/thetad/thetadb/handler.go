package thetadb

// Handlerthetadb is a synthetic struct.
type Handlerthetadb struct {
	ID   int
	Name string
}

// Newthetadb returns a new handler.
func Newthetadb() *Handlerthetadb {
	return &Handlerthetadb{ID: 1, Name: "thetadb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetadb) ProcessRequest(req string) string {
	return req
}
