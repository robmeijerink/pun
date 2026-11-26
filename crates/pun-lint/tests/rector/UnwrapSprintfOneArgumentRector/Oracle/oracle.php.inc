<?php

function sprintfOracle($a, $b, $f)
{
    $r = \sprintf($a);
    $r = SPRINTF(format: $a);
    $r = sprintf();
    $r = 'x' . sprintf($a . $b);
    $r = sprintf($a)[0];
    $r = $f($a);
    $r = printf($a);
}
