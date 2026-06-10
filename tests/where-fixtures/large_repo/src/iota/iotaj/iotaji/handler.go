package iotaji

// Handleriotaji is a synthetic struct.
type Handleriotaji struct {
	ID   int
	Name string
}

// Newiotaji returns a new handler.
func Newiotaji() *Handleriotaji {
	return &Handleriotaji{ID: 1, Name: "iotaji"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaji) ProcessRequest(req string) string {
	return req
}
