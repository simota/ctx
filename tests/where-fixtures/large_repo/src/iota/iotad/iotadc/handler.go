package iotadc

// Handleriotadc is a synthetic struct.
type Handleriotadc struct {
	ID   int
	Name string
}

// Newiotadc returns a new handler.
func Newiotadc() *Handleriotadc {
	return &Handleriotadc{ID: 1, Name: "iotadc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadc) ProcessRequest(req string) string {
	return req
}
