// Fixture: Go call-shaped syntax.
package fixture

func helper(a, b int) int {
	return a + b
}

func calls(obj *Widget) {
	helper(1, 2)
	obj.Method(1, 2)
	Map[int, string](items, transform)
	handlers[0](1, 2)
	getHandler()(1, 2)
	go helper(1, 2)
	defer helper(3, 4)
	converted := int64(5)
	_ = converted
}
