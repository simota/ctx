package iotajf

// Handleriotajf is a synthetic struct.
type Handleriotajf struct {
	ID   int
	Name string
}

// Newiotajf returns a new handler.
func Newiotajf() *Handleriotajf {
	return &Handleriotajf{ID: 1, Name: "iotajf"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotajf) ProcessRequest(req string) string {
	return req
}
