package iotafh

// Handleriotafh is a synthetic struct.
type Handleriotafh struct {
	ID   int
	Name string
}

// Newiotafh returns a new handler.
func Newiotafh() *Handleriotafh {
	return &Handleriotafh{ID: 1, Name: "iotafh"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotafh) ProcessRequest(req string) string {
	return req
}
