package engine

type Cache struct {
	m map[string]int
}

func (c *Cache) Get(k string) int {
	return c.m[k]
}

func (c *Cache) Set(k string, v int) {
	c.m[k] = v
}
