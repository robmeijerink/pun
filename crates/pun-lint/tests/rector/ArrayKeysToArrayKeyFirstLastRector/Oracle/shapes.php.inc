<?php

function keys(array $items, array $more)
{
    $a = \current(\array_keys($items));
    $b = END(Array_Keys($items));
    $c = reset(array_keys(array: $items));
    $d = current(array: array_keys($items));
    $e = end(array_keys(...$more));
    $f = key(array_keys($items));
    $h = end(ARRAY_KEYS($items));
    $g = current(array_keys($items), 1);
    define('A', 1, true);
    define('B', 2);
    \define('C', 3, false);
}
