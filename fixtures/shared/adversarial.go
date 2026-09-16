// Fixture: shared adversarial cases for Go.
//
// UTF-8 multibyte text before a declaration: "αβγ δεζ" and "日本語テキスト".
package adversarial

// Duplicate names in different contexts.

type Duplicate struct{}

func (d Duplicate) Name() string {
	return "method"
}

type other struct{}

func (o other) Name() string {
	return "other"
}

func Name() string {
	return "function"
}

// Deep nesting: closures, not declarations.

func deepNesting() {
	level1 := func() {
		level2 := func() {
			level3 := func() {
				level4 := func() {}
				level4()
			}
			level3()
		}
		level2()
	}
	level1()
}

// Syntactically valid but semantically invalid: `Missing` is never declared.

func semanticallyInvalid() Missing {
	value := AlsoMissing{}
	return value
}

// Generated-looking source.

func generated0001() {}
func generated0002() {}
func generated0003() {}
func generated0004() {}
func generated0005() {}
func generated0006() {}
func generated0007() {}
func generated0008() {}
