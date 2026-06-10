package iotaic

// Handleriotaic is a synthetic struct.
type Handleriotaic struct {
	ID   int
	Name string
}

// Newiotaic returns a new handler.
func Newiotaic() *Handleriotaic {
	return &Handleriotaic{ID: 1, Name: "iotaic"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaic) ProcessRequest(req string) string {
	return req
}
