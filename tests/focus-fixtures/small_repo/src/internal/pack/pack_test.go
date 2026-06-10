package pack

import "testing"

func TestPack(t *testing.T) {
	err := Pack(nil)
	if err != nil {
		t.Fatal(err)
	}
}
