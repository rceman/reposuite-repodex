<?php
namespace App\Worker;

use App\Repo\OrderRepo;
use App\Repo\UserRepo;

class SyncWorker {
    public function run(bool $full): array {
        if ($full) {
            $p = new OrderRepo();
        } else {
            $p = new UserRepo();
        }
        return $p->find(1);
    }
}
