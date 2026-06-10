package iotadf

// Handleriotadf is a synthetic struct.
type Handleriotadf struct {
	ID   int
	Name string
}

// Newiotadf returns a new handler.
func Newiotadf() *Handleriotadf {
	return &Handleriotadf{ID: 1, Name: "iotadf"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotadf) ProcessRequest(req string) string {
	return req
}
