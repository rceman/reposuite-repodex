<?php
namespace App\Service;

use function App\Util\slugify;

class CaseTest {
    public function check(): void {
        SLUGIFY('x');                  // case-insensitive function lookup
        new repo();                    // case-insensitive class lookup
        helper::RUN();                 // case-insensitive class+method lookup
    }
}
