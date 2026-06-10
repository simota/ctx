package iotaef

// Handleriotaef is a synthetic struct.
type Handleriotaef struct {
	ID   int
	Name string
}

// Newiotaef returns a new handler.
func Newiotaef() *Handleriotaef {
	return &Handleriotaef{ID: 1, Name: "iotaef"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaef) ProcessRequest(req string) string {
	return req
}
