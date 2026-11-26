<?php

declare(strict_types=1);

namespace ExceptionCodes;

use Exception;
use PDOException;
use RuntimeException;
use Throwable;

use function PHPStan\dumpType;

final class InvalidConfigurationException extends Exception
{
    public static function withContext(string $context, self $previous): self
    {
        return new self(
            sprintf('%s: %s', $context, $previous->getMessage()),
            $previous->getCode(),
            $previous,
        );
    }
}

class DomainFailure extends RuntimeException
{
}

final class QueryFailure extends PDOException
{
}

function takesInt(int $code): int
{
    return $code;
}

function codes(
    InvalidConfigurationException $final,
    DomainFailure $domain,
    \LogicException $logic,
    Exception $exception,
    RuntimeException $runtime,
    PDOException $pdo,
    QueryFailure $query,
    Throwable $throwable,
    \Error $error,
    DomainFailure|QueryFailure $either,
): void {
    dumpType($final->getCode());
    dumpType($domain->getCode());
    dumpType($logic->getCode());
    dumpType($exception->getCode());
    dumpType($runtime->getCode());
    dumpType($pdo->getCode());
    dumpType($query->getCode());
    dumpType($throwable->getCode());
    dumpType($error->getCode());
    dumpType($either->getCode());
    takesInt($exception->getCode());
    takesInt($pdo->getCode());
}
