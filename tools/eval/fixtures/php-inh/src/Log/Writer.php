<?php

namespace App\Log;

class Writer {
    public function save(array $row): int {
        return -2;
    }

    public function log(string $what): void {
        echo $what;
    }
}
