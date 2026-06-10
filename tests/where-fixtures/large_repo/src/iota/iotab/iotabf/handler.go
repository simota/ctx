package iotabf

// Handleriotabf is a synthetic struct.
type Handleriotabf struct {
	ID   int
	Name string
}

// Newiotabf returns a new handler.
func Newiotabf() *Handleriotabf {
	return &Handleriotabf{ID: 1, Name: "iotabf"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabf) ProcessRequest(req string) string {
	return req
}
