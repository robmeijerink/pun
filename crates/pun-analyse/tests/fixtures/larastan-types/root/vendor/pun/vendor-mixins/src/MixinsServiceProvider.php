<?php

namespace PunVendorMixins;

use Illuminate\Support\ServiceProvider;
use Illuminate\Support\Str;

class MixinsServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        Str::mixin(new StrMixins());
    }
}
