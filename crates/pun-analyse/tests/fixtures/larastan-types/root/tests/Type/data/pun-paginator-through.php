<?php

namespace PunPaginatorThrough;

use App\User;
use Illuminate\Contracts\Pagination\LengthAwarePaginator;

use function PHPStan\Testing\assertType;

final class Paginate
{
    /** @return LengthAwarePaginator<int, covariant array<string, mixed>> */
    public static function rows(): LengthAwarePaginator
    {
        return User::query()
            ->paginate(25)
            ->through(fn (User $user): array => ['id' => $user->id]);
    }

    /** @return LengthAwarePaginator<int, int> */
    public static function wrong(): LengthAwarePaginator
    {
        return User::query()
            ->paginate(25)
            ->through(fn (User $user): string => $user->name);
    }

    public static function simple(): void
    {
        assertType('Illuminate\Pagination\Paginator<int, string>', User::query()->simplePaginate(25)->through(fn (User $user): string => $user->name));
    }

    public static function cursor(): void
    {
        assertType('Illuminate\Pagination\CursorPaginator<int, string>', User::query()->cursorPaginate(25)->through(fn (User $user): string => $user->name));
    }

    public static function shape(): void
    {
        assertType('Illuminate\Pagination\LengthAwarePaginator<int, array{id: int}>', User::query()->paginate(25)->through(fn (User $user): array => ['id' => $user->id]));
    }
}
