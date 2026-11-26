<?php

namespace App\Providers;

use Illuminate\Support\ServiceProvider;

class PunViewsServiceProvider extends ServiceProvider
{
    public function boot(): void
    {
        $this->loadViewsFrom(__DIR__ . '/../../resources/pun-acme', 'punacme');
    }
}
