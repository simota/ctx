package iotacb

// Handleriotacb is a synthetic struct.
type Handleriotacb struct {
	ID   int
	Name string
}

// Newiotacb returns a new handler.
func Newiotacb() *Handleriotacb {
	return &Handleriotacb{ID: 1, Name: "iotacb"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotacb) ProcessRequest(req string) string {
	return req
}
