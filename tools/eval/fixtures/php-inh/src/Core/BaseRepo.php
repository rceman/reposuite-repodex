<?php

namespace App\Core;

abstract class BaseRepo {
    protected string $table = '';

    public function save(array $row): int {
        $this->log('save');
        return 1;
    }

    public function find(int $id): ?array {
        return ['id' => $id];
    }

    public function delete(int $id): bool {
        return true;
    }

    protected function log(string $what): void {
    }
}
