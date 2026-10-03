<?php

namespace App\Repo;

use App\Core\BaseRepo;

class UserRepo extends BaseRepo {
    protected string $table = 'users';

    public function save(array $row): int {
        $this->log('user-save');
        return parent::save($row) + 1;
    }
}
