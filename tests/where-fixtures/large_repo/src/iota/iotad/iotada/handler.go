package iotada

// Handleriotada is a synthetic struct.
type Handleriotada struct {
	ID   int
	Name string
}

// Newiotada returns a new handler.
func Newiotada() *Handleriotada {
	return &Handleriotada{ID: 1, Name: "iotada"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotada) ProcessRequest(req string) string {
	return req
}
