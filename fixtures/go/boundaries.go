//go:build linux && amd64

// Fixture: Go boundary probes.
package fixture

type Handler interface {
	Serve()
}

type Base struct{}

type Derived struct {
	Base
	Handler
}

func (b Base) Method() {}

// Tree-sitter's Go grammar decides between a conversion and a call from syntax
// alone. Observed shapes:
//
//   Generic[int](value)          -> type_conversion_expression (not call-like)
//   Generic[int](value, other)   -> call_expression / index_expression (indirect)
//   Generic[int, string](a, b)   -> call_expression / type_arguments (plain name)
//   int64(value)                 -> call_expression (reported; conversion or call)
func genericShapes(value, other int) {
	converted := Generic[int](value)
	oneTypeArg := Generic[int](value, other)
	twoTypeArgs := Generic[int, string](value, other)
	plainConverted := int64(value)
	_, _, _, _ = converted, oneTypeArg, twoTypeArgs, plainConverted
}

func methodExpression() {
	f := Base.Method
	_ = f
	var d Derived
	d.Method()
	d.Base.Method()
	count := 1
	{
		count := 2
		_ = count
	}
	_ = count
}
