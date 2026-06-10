package iotadb

// Handleriotadb is a synthetic struct.
type Handleriotadb struct {
	ID   int
	Name string
}

// Newiotadb returns a new handler.
func Newiotadb() *Handleriotadb {
	return &Handleriotadb{ID: 1, Name: "iotadb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadb) ProcessRequest(req string) string {
	return req
}
