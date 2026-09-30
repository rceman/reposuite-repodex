<?php
namespace Acme\App\Repo;

class Pages {
    public static function lookup(string $slug): ?string {
        return db_fetch('pages', $slug);
    }
    public function query(string $slug): ?string {
        return self::lookup($slug);
    }
}
