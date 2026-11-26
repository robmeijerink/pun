<?php

namespace PunAuthContracts;

use Illuminate\Contracts\Auth\Access\Authorizable;
use Illuminate\Contracts\Auth\Authenticatable;
use Illuminate\Contracts\Auth\CanResetPassword;

use function PHPStan\Testing\assertType;

final class Policy
{
    public function view(Authenticatable $user): bool
    {
        return $user->can('view-posts');
    }

    public function contracts(Authenticatable $user, CanResetPassword $reset, Authorizable $authorizable): void
    {
        assertType('bool', $user->can('view-posts'));
        assertType('bool', $reset->cannot('view-posts'));
        assertType('string', $authorizable->getEmailForPasswordReset());
        assertType('mixed', $user->getKey());
    }

    public function missing(Authenticatable $user): void
    {
        $user->doesNotExist();
    }

    public function property(Authenticatable $user): mixed
    {
        return $user->email;
    }
}
