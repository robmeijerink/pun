<?php

final class DeclaredStringClosureParam
{
    public function run(array $rows): array
    {
        return array_map(fn (string $a): string => (string) $a, $rows);
    }
}
-----
<?php

final class DeclaredStringClosureParam
{
    public function run(array $rows): array
    {
        return array_map(fn (string $a): string => $a, $rows);
    }
}
