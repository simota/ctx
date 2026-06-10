package iotaah

// Handleriotaah is a synthetic struct.
type Handleriotaah struct {
	ID   int
	Name string
}

// Newiotaah returns a new handler.
func Newiotaah() *Handleriotaah {
	return &Handleriotaah{ID: 1, Name: "iotaah"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaah) ProcessRequest(req string) string {
	return req
}
