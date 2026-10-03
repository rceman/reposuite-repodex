<?php
namespace App\Service;

class OrderService {
    public function handle(): string { return 'order'; }
    public function save(int $id): bool { return true; }
}
