package iotaag

// Handleriotaag is a synthetic struct.
type Handleriotaag struct {
	ID   int
	Name string
}

// Newiotaag returns a new handler.
func Newiotaag() *Handleriotaag {
	return &Handleriotaag{ID: 1, Name: "iotaag"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotaag) ProcessRequest(req string) string {
	return req
}
