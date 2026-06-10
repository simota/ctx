package iotaje

// Handleriotaje is a synthetic struct.
type Handleriotaje struct {
	ID   int
	Name string
}

// Newiotaje returns a new handler.
func Newiotaje() *Handleriotaje {
	return &Handleriotaje{ID: 1, Name: "iotaje"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaje) ProcessRequest(req string) string {
	return req
}
