package iotaad

// Handleriotaad is a synthetic struct.
type Handleriotaad struct {
	ID   int
	Name string
}

// Newiotaad returns a new handler.
func Newiotaad() *Handleriotaad {
	return &Handleriotaad{ID: 1, Name: "iotaad"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaad) ProcessRequest(req string) string {
	return req
}
