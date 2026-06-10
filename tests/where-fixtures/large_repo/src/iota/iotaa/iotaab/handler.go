package iotaab

// Handleriotaab is a synthetic struct.
type Handleriotaab struct {
	ID   int
	Name string
}

// Newiotaab returns a new handler.
func Newiotaab() *Handleriotaab {
	return &Handleriotaab{ID: 1, Name: "iotaab"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaab) ProcessRequest(req string) string {
	return req
}
