package iotagi

// Handleriotagi is a synthetic struct.
type Handleriotagi struct {
	ID   int
	Name string
}

// Newiotagi returns a new handler.
func Newiotagi() *Handleriotagi {
	return &Handleriotagi{ID: 1, Name: "iotagi"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotagi) ProcessRequest(req string) string {
	return req
}
