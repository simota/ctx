package iotafe

// Handleriotafe is a synthetic struct.
type Handleriotafe struct {
	ID   int
	Name string
}

// Newiotafe returns a new handler.
func Newiotafe() *Handleriotafe {
	return &Handleriotafe{ID: 1, Name: "iotafe"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotafe) ProcessRequest(req string) string {
	return req
}
