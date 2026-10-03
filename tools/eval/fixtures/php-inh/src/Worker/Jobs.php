<?php

namespace App\Worker;

use App\Service\OrderService;
use App\Service\ReportService;

// Distractor `run`; also hosts the static:: / trait negative cases.
class Jobs {
    public function run(OrderService $orders, ReportService $reports): void {
        $orders->process([]);          // inherited: BaseService::process -> handle
        $reports->generate();          // direct
        $orders::later();              // static:: OOS
        static::boot();                // static:: OOS
    }

    public static function boot(): void {
    }
}
