package iotaja

// Handleriotaja is a synthetic struct.
type Handleriotaja struct {
	ID   int
	Name string
}

// Newiotaja returns a new handler.
func Newiotaja() *Handleriotaja {
	return &Handleriotaja{ID: 1, Name: "iotaja"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaja) ProcessRequest(req string) string {
	return req
}
