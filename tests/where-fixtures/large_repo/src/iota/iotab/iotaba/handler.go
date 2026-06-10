package iotaba

// Handleriotaba is a synthetic struct.
type Handleriotaba struct {
	ID   int
	Name string
}

// Newiotaba returns a new handler.
func Newiotaba() *Handleriotaba {
	return &Handleriotaba{ID: 1, Name: "iotaba"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaba) ProcessRequest(req string) string {
	return req
}
