package iotabg

// Handleriotabg is a synthetic struct.
type Handleriotabg struct {
	ID   int
	Name string
}

// Newiotabg returns a new handler.
func Newiotabg() *Handleriotabg {
	return &Handleriotabg{ID: 1, Name: "iotabg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabg) ProcessRequest(req string) string {
	return req
}
