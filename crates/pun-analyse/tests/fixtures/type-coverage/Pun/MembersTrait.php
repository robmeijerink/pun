<?php

declare(strict_types=1);

namespace Pun\TypeCoverage;

trait MembersTrait
{
    const FROM_TRAIT = 1;

    public $fromTrait;

    public function fromTrait($value)
    {
        $inner = fn ($x) => $x;
    }
}
