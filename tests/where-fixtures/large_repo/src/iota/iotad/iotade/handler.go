package iotade

// Handleriotade is a synthetic struct.
type Handleriotade struct {
	ID   int
	Name string
}

// Newiotade returns a new handler.
func Newiotade() *Handleriotade {
	return &Handleriotade{ID: 1, Name: "iotade"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotade) ProcessRequest(req string) string {
	return req
}
