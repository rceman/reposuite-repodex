<?php
namespace Acme\TextLib;

use Acme\TextLib\Store\Repo;
use Acme\TextLib\Store\Cache;
use function Acme\TextLib\Text\truncate as cut;

class Support {
    public static function repo(): Repo { return new Repo(); }
    public function cache(): Cache { return Cache::make(); }
    public function fit(string $s): string { return cut($s, 8); }
}
