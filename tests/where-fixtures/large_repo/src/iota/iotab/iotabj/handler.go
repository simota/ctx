package iotabj

// Handleriotabj is a synthetic struct.
type Handleriotabj struct {
	ID   int
	Name string
}

// Newiotabj returns a new handler.
func Newiotabj() *Handleriotabj {
	return &Handleriotabj{ID: 1, Name: "iotabj"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabj) ProcessRequest(req string) string {
	return req
}
