<?php

namespace PunVendorMixinCalls;

use Illuminate\Support\Str;

use function PHPStan\Testing\assertType;

function counts(): void
{
    assertType('int', Str::punVendorCount('abc'));
}
