<?php
namespace Acme\TextLib\Store;

use Acme\TextLib\Text;
use function Acme\TextLib\Text\slugify;

class Repo {
    private array $rows = [];

    public static function empty(): self { return new self(); }

    public function put(string $key, string $value): string {
        $k = slugify($key);
        $this->write($k, $value);
        return $k;
    }

    public function lookup(string $key): ?string {
        $k = slugify($key);
        return $this->rows[$k] ?? null;
    }

    private function write(string $k, string $v): void {
        $this->rows[$k] = $v;
    }
}
