package iotaie

// Handleriotaie is a synthetic struct.
type Handleriotaie struct {
	ID   int
	Name string
}

// Newiotaie returns a new handler.
func Newiotaie() *Handleriotaie {
	return &Handleriotaie{ID: 1, Name: "iotaie"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaie) ProcessRequest(req string) string {
	return req
}
