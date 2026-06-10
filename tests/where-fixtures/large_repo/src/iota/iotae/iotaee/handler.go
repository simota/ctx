package iotaee

// Handleriotaee is a synthetic struct.
type Handleriotaee struct {
	ID   int
	Name string
}

// Newiotaee returns a new handler.
func Newiotaee() *Handleriotaee {
	return &Handleriotaee{ID: 1, Name: "iotaee"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaee) ProcessRequest(req string) string {
	return req
}
