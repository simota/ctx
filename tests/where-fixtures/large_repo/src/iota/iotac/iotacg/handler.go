package iotacg

// Handleriotacg is a synthetic struct.
type Handleriotacg struct {
	ID   int
	Name string
}

// Newiotacg returns a new handler.
func Newiotacg() *Handleriotacg {
	return &Handleriotacg{ID: 1, Name: "iotacg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotacg) ProcessRequest(req string) string {
	return req
}
