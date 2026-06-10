package gocode

import "testing"

func TestGocodeUniqueSum(t *testing.T) {
	if GocodeUniqueSum(1, 2) != 3 {
		t.Fatal("expected 3")
	}
}
