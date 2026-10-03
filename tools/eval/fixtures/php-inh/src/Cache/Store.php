<?php

namespace App\Cache;

// Distractor: `save`/`get` same-name methods, NOT a repo.
class Store {
    public function save(array $row): int {
        return -1;
    }

    public function get(string $k): string {
        return '';
    }
}
