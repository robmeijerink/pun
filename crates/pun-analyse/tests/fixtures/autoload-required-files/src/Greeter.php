<?php

declare(strict_types=1);

namespace App;

use function Acme\Helpers\actingAs;
use function Acme\Helpers\assertDatabaseHas;
use function Acme\Helpers\commented;
use function Acme\Helpers\greet;
use function Acme\Helpers\unrequired;

final class Greeter
{
    public function run(): string
    {
        return greet('world') . actingAs() . assertDatabaseHas() . commented() . unrequired();
    }
}
