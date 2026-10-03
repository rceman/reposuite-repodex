<?php

namespace App\Auth;

interface TokenVerifier {
    public function verify(string $token): bool;
}

trait Auditable {
    public function audited(): bool {
        return true;
    }
}

class Gate implements TokenVerifier {
    use Auditable;

    public function verify(string $token): bool {
        return true;
    }
}
