<?php declare(strict_types = 1);

namespace App;

use Mockery\LegacyMockInterface;
use Mockery\MockInterface;

use function PHPStan\dumpType;

final class Sealed
{
}

interface SubMock extends MockInterface
{
}

final class Intersections
{
    /**
     * @param MockInterface|Sealed $sealed
     * @param MockInterface|SubMock $sub
     * @param LegacyMockInterface|MockInterface $legacy
     * @param MockInterface|object $object
     * @param MockInterface|mixed $mixed
     * @param MockInterface|int $int
     * @param MockInterface|array<int> $array
     * @param MockInterface|callable $callable
     */
    public function params($sealed, $sub, $legacy, $object, $mixed, $int, $array, $callable): void
    {
        dumpType($sealed);
        dumpType($sub);
        dumpType($legacy);
        dumpType($object);
        dumpType($mixed);
        dumpType($int);
        dumpType($array);
        dumpType($callable);
    }

    public function staticStub(): void
    {
        dumpType(MockInterface::allows());
    }
}
