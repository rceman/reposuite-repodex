package fixture

import "testing"

func TestSomething(t *testing.T) {
	cases := []struct {
		name string
		want int
	}{
		{"a", 1},
		{"b", 2},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if tc.want == 0 {
				t.Fatal("zero")
			}
		})
	}
}

func TestHelperOnly(t *testing.T) {}

func Test_lowercase_after_prefix(t *testing.T) {}

func BenchmarkSomething(b *testing.B) {}

func helperTakingT(t *testing.T) {}

func Testify() {}

func helperPlain(value int) {}
