<?php

// Pun's stand-in for phpstan-mockery 2.0.0's src/Mockery/Type/Allows.php and Expects.php, with
// StubMethodsClassReflectionExtension: Pun gives each interface a copy of `__pun_stub()` for every method name the
// analysed files call, so every method exists, takes any arguments and returns `Mockery\Expectation`.

namespace PHPStan\Mockery\Type;

interface Allows
{

	/**
	 * @return \Mockery\Expectation
	 */
	public function __pun_stub(mixed ...$arguments);

}

interface Expects
{

	/**
	 * @return \Mockery\Expectation
	 */
	public function __pun_stub(mixed ...$arguments);

}
