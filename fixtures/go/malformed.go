// Fixture: Go recovery. Valid declarations surround damaged syntax.
package fixture

func validBefore() {}

func broken( {
	return
}

func validAfter() {}

var incomplete =
