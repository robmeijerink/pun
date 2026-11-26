<?php

declare(strict_types=1);

/**
 * @phpstan-type Level0 array{a: int, b: int}
 * @phpstan-type Level1 array{a: Level0, b: int}
 * @phpstan-type Level2 array{a: Level1, b: int}
 * @phpstan-type Level3 array{a: Level2, b: int}
 * @phpstan-type Level4 array{a: Level3, b: int}
 * @phpstan-type Level5 array{a: Level4, b: int}
 * @phpstan-type Level6 array{a: Level5, b: int}
 * @phpstan-type Level7 array{a: Level6, b: int}
 * @phpstan-type Level8 array{a: Level7, b: int}
 * @phpstan-type Level9 array{a: Level8, b: int}
 * @phpstan-type Level10 array{a: Level9, b: int}
 * @phpstan-type Level11 array{a: Level10, b: int}
 * @phpstan-type Level12 array{a: Level11, b: int}
 * @phpstan-type Level13 array{a: Level12, b: int}
 * @phpstan-type Level14 array{a: Level13, b: int}
 * @phpstan-type Level15 array{a: Level14, b: int}
 */
final class Repro
{
    /** @return Level15 */
    public function build(): array
    {
        throw new \RuntimeException('never');
    }

    /** @return Level15 */
    public function reported(): int
    {
        throw new \RuntimeException('never');
    }
}
