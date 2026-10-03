<?php
namespace App\Util;

class Str {
    public function slug(string $s): string { return strtolower($s); }
}
