<?php
namespace App\Worker;

class LegacyWorker {
    public function run($job): string {
        return $job->handle();
    }

    public function dispatch($job, string $name): string {
        return $job->{$name}();
    }
}
