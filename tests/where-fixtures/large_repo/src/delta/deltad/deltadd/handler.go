package deltadd

// Handlerdeltadd is a synthetic struct.
type Handlerdeltadd struct {
	ID   int
	Name string
}

// Newdeltadd returns a new handler.
func Newdeltadd() *Handlerdeltadd {
	return &Handlerdeltadd{ID: 1, Name: "deltadd"}
}

// ProcessRequest handles incoming requests with relevance scoring.
func (h *Handlerdeltadd) ProcessRequest(req string) string {
	return req
}
