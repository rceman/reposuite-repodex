package engine

import "example.com/micro/internal/util"

type Resolver struct {
	cache *Cache
}

func NewResolver() *Resolver {
	return &Resolver{cache: &Cache{m: map[string]int{}}}
}

func (r *Resolver) ResolveKey(k string) int {
	enc := util.Encode(k)
	if v, ok := r.cache.m[enc]; ok {
		return v
	}
	return -1
}
