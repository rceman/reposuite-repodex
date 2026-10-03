<?php

namespace App\Core;

abstract class BaseService {
    public function handle(array $job): bool {
        $this->audit($job);
        return true;
    }

    public function process(array $job): void {
        $this->handle($job);
    }

    protected function audit(array $job): void {
    }
}
