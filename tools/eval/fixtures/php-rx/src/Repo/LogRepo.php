<?php
namespace App\Repo;

class LogRepo {
    public function save(string $line): bool { return true; }
    public function write(string $line): void {}
}
