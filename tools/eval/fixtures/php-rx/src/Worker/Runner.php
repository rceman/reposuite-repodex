<?php
namespace App\Worker;

use App\Service\JobService;
use App\Service\ReportService;

class Runner {
    public function run(JobService $job): string {
        return $job->handle();
    }

    public function report(): string {
        $r = new ReportService();
        return $r->generate();
    }
}
