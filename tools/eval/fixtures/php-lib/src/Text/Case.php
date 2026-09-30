<?php
namespace Acme\TextLib\Text;

function upper(string $s): string { return strtoupper($s); }

class Shout {
    public static function of(string $s): string { return upper($s) . '!'; }
}
