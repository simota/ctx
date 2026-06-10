package iotaac

// Handleriotaac is a synthetic struct.
type Handleriotaac struct {
	ID   int
	Name string
}

// Newiotaac returns a new handler.
func Newiotaac() *Handleriotaac {
	return &Handleriotaac{ID: 1, Name: "iotaac"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaac) ProcessRequest(req string) string {
	return req
}
