<?php

namespace PunNullableModelColumns;

use App\User;
use Illuminate\Support\Collection;

use function PHPStan\Testing\assertType;

final class Reads
{
    /** @param Collection<int, User> $users */
    public function columns(Collection $users, ?User $user, User $plain): void
    {
        assertType('App\User|null', $users->first());
        assertType('string', $users->first()->name);
        assertType('string', $user->name);
        assertType('string|null', $user?->name);
        assertType('string', $plain->name);
    }

    /** @param Collection<int, User> $users */
    public static function firstName(Collection $users): int
    {
        return $users->first()->name;
    }
}
