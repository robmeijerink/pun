<?php

namespace PunVendorMixins;

class StrMixins
{
    public function punVendorCount(): \Closure
    {
        return fn (string $value): int => strlen($value);
    }
}
