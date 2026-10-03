<?php

namespace App\Controller;

use App\Core\Controller;
use App\Service\OrderService;

class OrderController extends Controller {
    public function store(OrderService $svc, array $job): string {
        $ok = $svc->handle($job);
        return $this->render('order', ['ok' => $ok]);
    }

    public function gate(): bool {
        return $this->authenticate();
    }
}
