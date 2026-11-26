<?php

declare(strict_types=1);

namespace FilterProvidersBoolean;

use function PHPStan\dumpType;

/**
 * @param 0|134217728 $nullFlag
 * @param array<string, mixed> $map
 * @param 257|258 $intOrBoolFilter
 */
function booleanCases(mixed $input, bool $bool, int $flags, int $nullFlag, array $map, ?bool $nullableBool, int $filter, int $intOrBoolFilter): void
{
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOL));
    dumpType(filter_var($input, 258));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, 0));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, ['flags' => FILTER_NULL_ON_FAILURE]));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, ['flags' => 0]));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, $nullFlag));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, $flags));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, ['options' => ['default' => 'bad']]));
    dumpType(filter_var($bool, FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_var($bool, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE));
    dumpType(filter_var(true, FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_var(false, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE));
    dumpType(filter_var(null, FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_var(null, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE));
    dumpType(filter_var(new \stdClass(), FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_var($map, FILTER_VALIDATE_BOOLEAN, FILTER_REQUIRE_ARRAY));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, FILTER_FORCE_ARRAY));
    dumpType(filter_input(INPUT_GET, 'flag', FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_input(INPUT_GET, 'flag', FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE));
    dumpType(filter_var($nullableBool, FILTER_VALIDATE_BOOLEAN));
    dumpType(filter_var($nullableBool, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, ['options' => ['default' => null]]));
    dumpType(filter_var($input, $filter));
    dumpType(filter_var($input, $intOrBoolFilter));
    dumpType(filter_var($input, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE | FILTER_REQUIRE_ARRAY));
}

final class Config
{
    public bool $cacheEnabled = false;

    /** @param array<string, mixed> $config */
    public static function fromArray(array $config): self
    {
        $instance = new self();

        if (isset($config['cache_enabled'])) {
            $instance->cacheEnabled = is_bool($config['cache_enabled'])
                ? $config['cache_enabled']
                : filter_var($config['cache_enabled'], FILTER_VALIDATE_BOOLEAN);
        }

        return $instance;
    }
}
