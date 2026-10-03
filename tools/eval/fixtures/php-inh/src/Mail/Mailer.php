<?php

namespace App\Mail;

// Distractor: `handle`/`process` same names, NOT a service.
class Mailer {
    public function handle(array $job): bool {
        return false;
    }

    public function process(array $job): void {
    }
}
