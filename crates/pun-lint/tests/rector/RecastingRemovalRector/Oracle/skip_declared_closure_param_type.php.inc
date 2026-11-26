<?php

declare(strict_types=1);

final class DeclaredClosureParamType
{
    private const string NONE = '';

    public function totals(array $rows): array
    {
        $totals = [];
        foreach ($rows as $row) {
            $totals[$this->key($row['id'] ?? null)] = 1;
        }

        uksort($totals, fn (int|string $a, int|string $b): int => ((string) $a === self::NONE) <=> ((string) $b === self::NONE)
            ?: strcmp((string) $a, (string) $b));
        usort($rows, function (int|string $a, int|string $b): int {
            return strcmp((string) $a, (string) $b);
        });

        return $totals;
    }

    public function keys(string $id): array
    {
        $totals = [$id => 1];

        return array_map(fn ($key) => (string) $key, array_keys($totals));
    }

    private function key(?string $id): string
    {
        return $id ?? self::NONE;
    }
}
