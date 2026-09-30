<?php
namespace Acme\TextLib\Tests;

use Acme\TextLib\Store\Repo;

class RepoTest {
    public function testPutLookup(): void {
        $repo = Repo::empty();
        $repo->put('Hello World', 'v');
        assert($repo->lookup('Hello World') === 'v');
    }
}
