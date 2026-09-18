<?php

namespace App\Service;

use App\Service\Foo;
use App\Other\Foo as OtherFoo;
use App\Dup\Thing;
use App\Missing\Thing;
use Symfony\Component\Console\Command\Command;
use App\Service\{Foo as GroupedFoo};
use function App\Service\helper;
use const App\Service\VERSION;

class Bar {}
