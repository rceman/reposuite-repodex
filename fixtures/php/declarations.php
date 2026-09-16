<?php

namespace App\Service;

use App\Contracts\Handler;
use App\Support\Logger as Log;
use App\Support\{Cache, Clock as C};
use function App\Support\helper;
use const App\Support\VERSION;

interface HandlerInterface
{
    public const VERSION = '1';

    public function handle(string $value): string;
}

trait Loggable
{
    public function log(string $message): void
    {
    }
}

abstract class BaseService implements HandlerInterface
{
    use Loggable;

    public const DEFAULT_NAME = 'base';
    protected static ?string $cacheKey = null;
    public readonly int $version;

    public function __construct(private string $name, protected int $limit = 10)
    {
        $this->version = 1;
    }

    abstract public function handle(string $value): string;
}

final class Service extends BaseService
{
    public static function make(string $name): static
    {
        return new static($name);
    }

    public function handle(string $value): string
    {
        return $value;
    }
}

enum Status: string
{
    case Active = 'active';
    case Closed = 'closed';

    public function label(): string
    {
        return $this->value;
    }
}

function helperFunction(): void
{
}
