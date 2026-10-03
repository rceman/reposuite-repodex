<?php
namespace App\Service;

class UserService {
    public function handle(): string { return 'user'; }
    public function save(int $id): bool { return true; }
}
