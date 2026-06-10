package iotafd

// Handleriotafd is a synthetic struct.
type Handleriotafd struct {
	ID   int
	Name string
}

// Newiotafd returns a new handler.
func Newiotafd() *Handleriotafd {
	return &Handleriotafd{ID: 1, Name: "iotafd"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotafd) ProcessRequest(req string) string {
	return req
}
