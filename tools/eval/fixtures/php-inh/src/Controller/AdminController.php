<?php

namespace App\Controller;

use App\Core\Controller;
use App\Repo\UserRepo;

class AdminController extends Controller {
    public function index(UserRepo $users): string {
        $users->save(['name' => 'root']);
        return $this->render('admin', []);
    }
}
