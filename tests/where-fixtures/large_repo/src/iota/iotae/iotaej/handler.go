package iotaej

// Handleriotaej is a synthetic struct.
type Handleriotaej struct {
	ID   int
	Name string
}

// Newiotaej returns a new handler.
func Newiotaej() *Handleriotaej {
	return &Handleriotaej{ID: 1, Name: "iotaej"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaej) ProcessRequest(req string) string {
	return req
}
