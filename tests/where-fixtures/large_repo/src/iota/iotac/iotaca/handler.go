package iotaca

// Handleriotaca is a synthetic struct.
type Handleriotaca struct {
	ID   int
	Name string
}

// Newiotaca returns a new handler.
func Newiotaca() *Handleriotaca {
	return &Handleriotaca{ID: 1, Name: "iotaca"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaca) ProcessRequest(req string) string {
	return req
}
