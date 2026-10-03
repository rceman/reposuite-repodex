<?php
namespace App\Controller;

use App\Cache\Cache;
use App\Repo\LogRepo;
use App\Service\UserService;

class UserController {
    public function __construct(private UserService $users, private Cache $store) {
        $this->audit = new LogRepo();
    }

    public function list(): string {
        $this->users->handle();
        $this->store->get('users');
        $this->audit->save('listed');
        return 'ok';
    }
}
