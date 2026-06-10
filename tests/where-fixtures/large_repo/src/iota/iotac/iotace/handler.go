package iotace

// Handleriotace is a synthetic struct.
type Handleriotace struct {
	ID   int
	Name string
}

// Newiotace returns a new handler.
func Newiotace() *Handleriotace {
	return &Handleriotace{ID: 1, Name: "iotace"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotace) ProcessRequest(req string) string {
	return req
}
