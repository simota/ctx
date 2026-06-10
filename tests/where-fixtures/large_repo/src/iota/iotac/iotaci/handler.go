package iotaci

// Handleriotaci is a synthetic struct.
type Handleriotaci struct {
	ID   int
	Name string
}

// Newiotaci returns a new handler.
func Newiotaci() *Handleriotaci {
	return &Handleriotaci{ID: 1, Name: "iotaci"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaci) ProcessRequest(req string) string {
	return req
}
