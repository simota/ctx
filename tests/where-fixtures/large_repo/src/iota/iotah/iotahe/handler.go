package iotahe

// Handleriotahe is a synthetic struct.
type Handleriotahe struct {
	ID   int
	Name string
}

// Newiotahe returns a new handler.
func Newiotahe() *Handleriotahe {
	return &Handleriotahe{ID: 1, Name: "iotahe"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotahe) ProcessRequest(req string) string {
	return req
}
