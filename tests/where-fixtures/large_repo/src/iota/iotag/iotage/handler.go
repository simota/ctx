package iotage

// Handleriotage is a synthetic struct.
type Handleriotage struct {
	ID   int
	Name string
}

// Newiotage returns a new handler.
func Newiotage() *Handleriotage {
	return &Handleriotage{ID: 1, Name: "iotage"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotage) ProcessRequest(req string) string {
	return req
}
