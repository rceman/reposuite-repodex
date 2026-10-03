<?php
namespace App\Controller;

use App\Cache\Cache;
use App\Page\PageRenderer;
use App\Repo\OrderRepo;
use App\Service\OrderService;

class OrderController {
    private OrderService $orders;

    public function __construct(private Cache $cache, OrderService $orders) {
        $this->orders = $orders;
        $this->repo = new OrderRepo();
        $this->pages = new PageRenderer();
    }

    public function index(): string {
        $this->orders->handle();
        return $this->pages->renderHome();
    }

    public function store(int $id): bool {
        return $this->repo->save($id);
    }

    public function warm(): ?string {
        return $this->cache->get('landing');
    }
}
