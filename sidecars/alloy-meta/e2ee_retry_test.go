package main

import (
	"testing"

	"go.mau.fi/whatsmeow"
)

func TestNeedsE2EEPreKeyTopUp(t *testing.T) {
	cases := []struct {
		name   string
		server int
		want   bool
	}{
		{"empty server", 0, true},
		{"below whatsmeow minimum", whatsmeow.MinPreKeyCount - 1, true},
		{"above whatsmeow minimum but below target", whatsmeow.MinPreKeyCount + 10, true},
		{"one below target", whatsmeow.WantedPreKeyCount - 1, true},
		{"at target", whatsmeow.WantedPreKeyCount, false},
		{"above target", whatsmeow.WantedPreKeyCount + 1, false},
		{"live server count", 794, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := needsE2EEPreKeyTopUp(tc.server); got != tc.want {
				t.Fatalf("needsE2EEPreKeyTopUp(%d) = %v, want %v", tc.server, got, tc.want)
			}
		})
	}
}
