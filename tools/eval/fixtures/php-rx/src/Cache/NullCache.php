<?php
namespace App\Cache;

class NullCache {
    public function get(string $k): ?string { return null; }
    public function put(string $k, string $v): void {}
}
