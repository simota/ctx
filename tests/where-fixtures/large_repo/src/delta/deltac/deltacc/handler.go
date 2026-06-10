package deltacc

// Handlerdeltacc is a synthetic struct.
type Handlerdeltacc struct {
	ID   int
	Name string
}

// Newdeltacc returns a new handler.
func Newdeltacc() *Handlerdeltacc {
	return &Handlerdeltacc{ID: 1, Name: "deltacc"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerdeltacc) ProcessRequest(req string) string {
	return req
}
