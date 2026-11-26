<?php

declare(strict_types=1);

namespace Pun\TypeCoverage;

final class FirstUser
{
    use MembersTrait;
}

final class SecondUser
{
    use MembersTrait;
}

final class Hooked
{
    public $name {
        set($value) {
            $this->name = $value;
        }
    }
}
