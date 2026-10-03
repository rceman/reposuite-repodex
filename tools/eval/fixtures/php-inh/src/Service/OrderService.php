<?php

namespace App\Service;

use App\Core\BaseService;
use App\Repo\OrderRepo;

class OrderService extends BaseService {
    private OrderRepo $orders;

    public function __construct() {
        $this->orders = new OrderRepo();
    }

    public function handle(array $job): bool {
        parent::handle($job);
        return $this->orders->save($job) > 0;
    }
}
