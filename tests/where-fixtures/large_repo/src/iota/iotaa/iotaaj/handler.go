package iotaaj

// Handleriotaaj is a synthetic struct.
type Handleriotaaj struct {
	ID   int
	Name string
}

// Newiotaaj returns a new handler.
func Newiotaaj() *Handleriotaaj {
	return &Handleriotaaj{ID: 1, Name: "iotaaj"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaaj) ProcessRequest(req string) string {
	return req
}
