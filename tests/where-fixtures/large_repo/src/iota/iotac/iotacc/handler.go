package iotacc

// Handleriotacc is a synthetic struct.
type Handleriotacc struct {
	ID   int
	Name string
}

// Newiotacc returns a new handler.
func Newiotacc() *Handleriotacc {
	return &Handleriotacc{ID: 1, Name: "iotacc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotacc) ProcessRequest(req string) string {
	return req
}
