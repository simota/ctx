package iotaia

// Handleriotaia is a synthetic struct.
type Handleriotaia struct {
	ID   int
	Name string
}

// Newiotaia returns a new handler.
func Newiotaia() *Handleriotaia {
	return &Handleriotaia{ID: 1, Name: "iotaia"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaia) ProcessRequest(req string) string {
	return req
}
