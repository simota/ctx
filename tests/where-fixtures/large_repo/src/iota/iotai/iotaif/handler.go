package iotaif

// Handleriotaif is a synthetic struct.
type Handleriotaif struct {
	ID   int
	Name string
}

// Newiotaif returns a new handler.
func Newiotaif() *Handleriotaif {
	return &Handleriotaif{ID: 1, Name: "iotaif"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaif) ProcessRequest(req string) string {
	return req
}
