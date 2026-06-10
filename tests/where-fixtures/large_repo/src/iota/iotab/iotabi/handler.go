package iotabi

// Handleriotabi is a synthetic struct.
type Handleriotabi struct {
	ID   int
	Name string
}

// Newiotabi returns a new handler.
func Newiotabi() *Handleriotabi {
	return &Handleriotabi{ID: 1, Name: "iotabi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabi) ProcessRequest(req string) string {
	return req
}
