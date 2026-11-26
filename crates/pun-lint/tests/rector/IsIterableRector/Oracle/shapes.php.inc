<?php

namespace App;

function go($foo)
{
    $a = $foo instanceof \Traversable || is_array($foo);
    $b = is_array($foo) || $foo instanceof Traversable;
    $c = is_array($foo->items()) || $foo->items() instanceof \Traversable;
}
-----
<?php

namespace App;

function go($foo)
{
    $a = is_iterable($foo);
    $b = is_array($foo) || $foo instanceof Traversable;
    $c = is_array($foo->items()) || $foo->items() instanceof \Traversable;
}
