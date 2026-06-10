package kappaja

// Handlerkappaja is a synthetic struct.
type Handlerkappaja struct {
	ID   int
	Name string
}

// Newkappaja returns a new handler.
func Newkappaja() *Handlerkappaja {
	return &Handlerkappaja{ID: 1, Name: "kappaja"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerkappaja) ProcessRequest(req string) string {
	return req
}
