<?php

declare(strict_types=1);

namespace UnpackedAppends;

use function PHPStan\dumpType;

final class LoopShift
{
    /**
     * @param  array<string, list<string>>  $tree
     * @return list<string>
     */
    public static function walk(array $tree): array
    {
        $seen = [];
        $pending = ['root'];

        while ($pending !== []) {
            $node = array_shift($pending);
            $seen[] = $node;
            array_unshift($pending, ...($tree[$node] ?? []));
            dumpType($pending);
        }

        return $seen;
    }

    /**
     * @param  list<string>  $more
     * @param  non-empty-list<string>  $some
     */
    public static function appends(array $more, array $some, string $one): void
    {
        $a = [];
        array_unshift($a, ...$more);
        dumpType($a);

        $b = [];
        array_push($b, ...$more);
        dumpType($b);

        $c = [];
        array_push($c, ...$some);
        dumpType($c);

        $d = [];
        array_unshift($d, $one, ...$more);
        dumpType($d);

        $e = ['x'];
        array_shift($e);
        array_unshift($e, ...$more);
        dumpType($e);

        $f = ['x'];
        array_push($f, ...$more);
        dumpType($f);
    }
}
