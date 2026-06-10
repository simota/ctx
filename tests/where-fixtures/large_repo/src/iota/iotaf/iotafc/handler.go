package iotafc

// Handleriotafc is a synthetic struct.
type Handleriotafc struct {
	ID   int
	Name string
}

// Newiotafc returns a new handler.
func Newiotafc() *Handleriotafc {
	return &Handleriotafc{ID: 1, Name: "iotafc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotafc) ProcessRequest(req string) string {
	return req
}
