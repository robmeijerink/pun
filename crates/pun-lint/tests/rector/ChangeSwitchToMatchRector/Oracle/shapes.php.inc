<?php

namespace Pun\Oracle\SwitchToMatch;

function &byRef($v)
{
    switch ($v) {
        case 1:
            return $v;
        default:
            return $v;
    }
}

function &byRefClosure()
{
    $f = function ($v) {
        switch ($v) {
            case 1:
                return 1;
            default:
                return 2;
        }
    };
    return $f;
}

function colon($v)
{
    switch ($v):
        case 1;
            return 'a';
        default:
            return 'b';
    endswitch;
}

function nested($v)
{
    if ($v) {
        switch (($v)) {
            case (1):
                return fn () => [
                    1,
                ];
            default:
                return foo(
                    "a
b"
                );
        }
    }
}

function castPrec(string $a, string $b)
{
    switch ($a . $b) {
        case 1:
            return 'x';
        default:
            return 'y';
    }
}

function boolDyn($v)
{
    switch ($v) {
        case $v > 1:
            return 'a';
        case is_int($v):
            return 'c';
        default:
            return 'b';
    }
}

function boolTrue($v)
{
    switch (true) {
        case $v > 1:
            return 'a';
        default:
            return 'b';
    }
}

function onlyComment($v)
{
    switch ($v) {
        case 4:
            $p = 'a';
            break;
        case 5:
            // fall
        default:
            $p = 'b';
            break;
    }
    return $p;
}

function emptyAll($v)
{
    switch ($v) {
        case 4:
        default:
            $p = 'b';
            break;
    }
    return $p;
}

function dimAssign($v)
{
    switch ($v) {
        case 1:
            $a['x'] = 1;
            break;
        default:
            $a['x'] = 2;
    }
    return $a;
}

function normalNext($v)
{
    switch ($v) {
        case 1:
            foo();
            break;
    }
    return bar();
}
-----
<?php

namespace Pun\Oracle\SwitchToMatch;

function &byRef($v)
{
    switch ($v) {
        case 1:
            return $v;
        default:
            return $v;
    }
}

function &byRefClosure()
{
    $f = function ($v) {
        return match ($v) {
            1 => 1,
            default => 2,
        };
    };
    return $f;
}

function colon($v)
{
    return match ($v) {
        1 => 'a',
        default => 'b',
    };
}

function nested($v)
{
    if ($v) {
        return match ($v) {
            1 => fn () => [
                1,
            ],
            default => foo(
                "a
b"
            ),
        };
    }
}

function castPrec(string $a, string $b)
{
    switch ($a . $b) {
        case 1:
            return 'x';
        default:
            return 'y';
    }
}

function boolDyn($v)
{
    return match (true) {
        $v > 1 => 'a',
        is_int($v) => 'c',
        default => 'b',
    };
}

function boolTrue($v)
{
    return match (true) {
        $v > 1 => 'a',
        default => 'b',
    };
}

function onlyComment($v)
{
    $p = match ($v) {
        4 => 'a',
        default => 'b',
    };
    return $p;
}

function emptyAll($v)
{
    $p = match ($v) {
        default => 'b',
    };
    return $p;
}

function dimAssign($v)
{
    $a['x'] = match ($v) {
        1 => 1,
        default => 2,
    };
    return $a;
}

function normalNext($v)
{
    switch ($v) {
        case 1:
            foo();
            break;
    }
    return bar();
}
