package iotacj

// Handleriotacj is a synthetic struct.
type Handleriotacj struct {
	ID   int
	Name string
}

// Newiotacj returns a new handler.
func Newiotacj() *Handleriotacj {
	return &Handleriotacj{ID: 1, Name: "iotacj"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handleriotacj) ProcessRequest(req string) string {
	return req
}
