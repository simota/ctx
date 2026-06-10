package iotaeg

// Handleriotaeg is a synthetic struct.
type Handleriotaeg struct {
	ID   int
	Name string
}

// Newiotaeg returns a new handler.
func Newiotaeg() *Handleriotaeg {
	return &Handleriotaeg{ID: 1, Name: "iotaeg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaeg) ProcessRequest(req string) string {
	return req
}
