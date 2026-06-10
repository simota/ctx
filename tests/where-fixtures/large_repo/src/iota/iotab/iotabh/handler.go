package iotabh

// Handleriotabh is a synthetic struct.
type Handleriotabh struct {
	ID   int
	Name string
}

// Newiotabh returns a new handler.
func Newiotabh() *Handleriotabh {
	return &Handleriotabh{ID: 1, Name: "iotabh"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotabh) ProcessRequest(req string) string {
	return req
}
