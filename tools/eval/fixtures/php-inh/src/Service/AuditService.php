<?php

namespace App\Service;

class AuditService extends OrderService {
    public function trail(array $job): bool {
        // OrderService::handle wins over BaseService::handle
        return $this->handle($job);
    }
}
