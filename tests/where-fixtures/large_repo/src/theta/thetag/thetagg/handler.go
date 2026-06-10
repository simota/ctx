package thetagg

// Handlerthetagg is a synthetic struct.
type Handlerthetagg struct {
	ID   int
	Name string
}

// Newthetagg returns a new handler.
func Newthetagg() *Handlerthetagg {
	return &Handlerthetagg{ID: 1, Name: "thetagg"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerthetagg) ProcessRequest(req string) string {
	return req
}
