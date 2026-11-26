#!/usr/bin/env python3
"""Write crates/pun-cli/tests/fixtures/outdated.json, the manifest of crates/pun-cli/tests/outdated.rs: small projects
with what `composer outdated` and `composer show --latest` 2.11.2 (.oracle/composer, the Composer the plugin host pins)
print and return for them, and the platform they ran on.

The latest versions come from the project's repositories. So that no case reaches the network, every root turns
packagist.org off and lists the versions there are in inline `package` repositories. A version newer than what is
installed may need a platform the PHP doesn't have, so the manifest records that platform once, as Pun's own probe (the
PROBE script of crates/pun-packages/src/resolver/platform.rs) run on the same `php`, without its `ini` and `inputs`
(local paths); the test gives Pun that probe through PHP_BINARY, a script that prints it. Release times are far in the
past or the future, so the cooldown policy decides the same on any day; what Composer says about how long ago or how
long from now (`26 years ago`, `26340d 23h left`) the test compares without its digits.

Each case is a set of files and the words after `composer`, run in a fresh temporary directory with a fresh HOME and
COMPOSER_HOME, COMPOSER_ROOT_VERSION=1.0.0 and COLUMNS=500 (unless the case's `env` says otherwise; a null leaves a
variable out), without a terminal on stdin. A case records `exit`, `stdout` and `stderr`, and `message` (the lines of
Composer's exception block) when Composer stops with an exception, which Pun prints as its own run error (exit 2).

usage: scripts/export-outdated-cases.py   (needs php 8.5 and .oracle/composer from scripts/oracle.sh)
"""
import json, os, re, shutil, subprocess, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PHAR = os.path.join(ROOT, '.oracle/composer/composer.phar')
OUT = os.path.join(ROOT, 'crates/pun-cli/tests/fixtures/outdated.json')
PLATFORM_RS = os.path.join(ROOT, 'crates/pun-packages/src/resolver/platform.rs')

OLD_SHA = '1111111111111111111111111111111111111111'
NEW_SHA = '2222222222222222222222222222222222222222'
PAST = '2000-01-01T00:00:00+00:00'
FUTURE = '2099-01-01T00:00:00+00:00'


def j(value):
    return json.dumps(value, indent=4) + '\n'


def package(name, version, **fields):
    normalized = re.sub(r'^v', '', version)
    if not (normalized.startswith('dev-') or '-' in normalized):
        normalized += '.0' * (3 - normalized.count('.'))
    p = {'name': name, 'version': version, 'version_normalized': fields.pop('version_normalized', normalized), 'type': 'library'}
    p.update({k.replace('_', '-'): v for k, v in fields.items()})
    return p


def installed(*packages, dev=()):
    """vendor/composer/installed.json and each package's directory (Composer drops a package whose directory isn't
    there)."""
    files = {'vendor/composer/installed.json': j({'packages': list(packages), 'dev': True, 'dev-package-names': list(dev)})}
    for p in packages:
        files[f'vendor/{p["name"].lower()}/.keep'] = ''
    return files


def lock(packages=(), packages_dev=()):
    return j({'content-hash': 'x', 'packages': list(packages), 'packages-dev': list(packages_dev)})


INSTALLED = [
    package('acme/alpha', '1.0.0', description='Patch, minor and major updates', require={'acme/gamma': '^0.3'}),
    package('acme/beta', '1.2.0', description='Up to date'),
    package('acme/gamma', '0.3.0', description='A transitive 0.x dependency'),
    package('acme/epsilon', 'dev-main', version_normalized='dev-main', description='A branch with a newer commit',
            source={'type': 'git', 'url': 'https://example.org/epsilon.git', 'reference': OLD_SHA}),
    package('acme/old', '1.0.0', description='Its newer version is abandoned'),
    package('acme/cool', '1.0.0', description='Its newer version is still cooling down'),
    package('acme/stable', '1.0.0', description='Only a beta is newer'),
    package('acme/delta', '1.0.0', description='A dev dependency whose newer version needs PHP 99'),
]
# The installed packages have no release time; the versions there are have one (the cooldown policy needs it).
AVAILABLE = [
    package('acme/alpha', '1.0.1', require={'acme/gamma': '^0.3'}),
    package('acme/alpha', '1.1.0', require={'acme/gamma': '^0.3'}),
    package('acme/alpha', '2.0.0', require={'acme/gamma': '^0.4'}),
    package('acme/beta', '1.2.0'),
    package('acme/gamma', '0.3.1'),
    package('acme/gamma', '0.4.0'),
    package('acme/epsilon', 'dev-main', version_normalized='dev-main',
            source={'type': 'git', 'url': 'https://example.org/epsilon.git', 'reference': NEW_SHA}),
    package('acme/old', '1.1.0', abandoned='acme/new'),
    package('acme/cool', '1.1.0', time=FUTURE),
    package('acme/stable', '1.1.0-beta1', version_normalized='1.1.0.0-beta1'),
    package('acme/delta', '1.5.0', require={'php': '>=99'}),
]
for p in AVAILABLE:
    p.setdefault('time', PAST)
# One repository with every version: Composer takes a package from the first repository that has it.
REPOSITORIES = [{'packagist.org': False}, {'type': 'package', 'package': AVAILABLE}]
ROOT_JSON = {'name': 'acme/app', 'require': {'acme/alpha': '^1.0', 'acme/beta': '^1.0', 'acme/epsilon': 'dev-main',
                                             'acme/old': '*', 'acme/cool': '*', 'acme/stable': '*'},
             'require-dev': {'acme/delta': '^1.0'}, 'repositories': REPOSITORIES}
COOLDOWN = {**ROOT_JSON, 'config': {'policy': {'cooldown': {'period': '7 days'}}}}


def vendor_project(manifest=None, packages=INSTALLED):
    return {'composer.json': j(ROOT_JSON if manifest is None else manifest), **installed(*packages, dev=['acme/delta'])}


def lock_project():
    return {'composer.json': j(ROOT_JSON), 'composer.lock': lock(INSTALLED[:-1], INSTALLED[-1:])}


CASES = [
    # outdated: the packages with a newer version.
    ('outdated', vendor_project(), ['outdated']),
    ('outdated-all', vendor_project(), ['outdated', '--all']),
    ('outdated-direct', vendor_project(), ['outdated', '-D']),
    ('outdated-strict', vendor_project(), ['outdated', '--strict']),
    ('outdated-strict-up-to-date', vendor_project(), ['outdated', '--strict', 'acme/beta']),
    ('outdated-major-only', vendor_project(), ['outdated', '-M']),
    ('outdated-minor-only', vendor_project(), ['outdated', '--minor-only']),
    ('outdated-patch-only', vendor_project(), ['outdated', '-p']),
    ('outdated-only-two', vendor_project(), ['outdated', '-m', '-p']),
    ('outdated-ignore', vendor_project(), ['outdated', '--ignore=acme/alpha', '--ignore', 'acme/g*']),
    ('outdated-json', vendor_project(), ['outdated', '--format=json']),
    ('outdated-all-json', vendor_project(), ['outdated', '-a', '-f', 'json']),
    ('outdated-no-dev', vendor_project(), ['outdated', '--no-dev']),
    ('outdated-ignore-platform-reqs', vendor_project(), ['outdated', '--ignore-platform-reqs']),
    ('outdated-ignore-platform-req', vendor_project(), ['outdated', '--ignore-platform-req=php']),
    ('outdated-locked', lock_project(), ['outdated', '--locked']),
    ('outdated-package', vendor_project(), ['outdated', 'acme/alpha']),
    ('outdated-pattern', vendor_project(), ['outdated', 'acme/[ag]*']),
    ('outdated-ansi', vendor_project(), ['outdated', '--ansi']),
    ('outdated-up-to-date', vendor_project(packages=INSTALLED[1:2]), ['outdated']),
    ('outdated-direct-up-to-date', vendor_project(packages=INSTALLED[1:3]), ['outdated', '-D']),
    ('outdated-nothing-installed', {'composer.json': j(ROOT_JSON)}, ['outdated']),
    # The cooldown policy.
    ('cooldown', vendor_project(manifest=COOLDOWN), ['outdated']),
    ('cooldown-all', vendor_project(manifest=COOLDOWN), ['outdated', '--all']),
    ('cooldown-period-zero', vendor_project(manifest=COOLDOWN), ['outdated', '--cooldown-period=0']),
    ('cooldown-period-option', vendor_project(), ['outdated', '--cooldown-period', '7 days']),
    ('cooldown-period-invalid', vendor_project(), ['outdated', '--cooldown-period=soon']),
    # show --latest and --outdated.
    ('show-latest', vendor_project(), ['show', '--latest']),
    ('show-latest-direct', vendor_project(), ['show', '-l', '-D']),
    ('show-latest-json', vendor_project(), ['show', '-l', '--format=json']),
    ('show-outdated', vendor_project(), ['show', '--outdated']),
    ('show-outdated-name-only', vendor_project(), ['show', '-o', '-N']),
    ('show-latest-package', vendor_project(), ['show', '-l', 'acme/alpha']),
    ('show-latest-package-json', vendor_project(), ['show', '-l', 'acme/gamma', '-f', 'json']),
    ('show-latest-tree', vendor_project(), ['show', '-l', '--tree']),
    ('show-ignore', vendor_project(), ['show', '-o', '--ignore=acme/*']),
    ('show-only-without-latest', vendor_project(), ['show', '-M', '-N']),
    # No composer.json.
    ('no-composer-json', {}, ['outdated']),
]


def composer(cwd, args, home, extra_env=None):
    env = {'PATH': os.environ['PATH'], 'HOME': home, 'COMPOSER_HOME': os.path.join(home, '.composer'),
           'COMPOSER_CACHE_DIR': os.path.join(home, '.cache'), 'COMPOSER_ROOT_VERSION': '1.0.0', 'COLUMNS': '500',
           **(extra_env or {})}
    env = {k: v for k, v in env.items() if v is not None}
    return subprocess.run(['php', PHAR, *args], cwd=cwd, env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL,
                          timeout=120)


def exception_message(stderr):
    """The lines of Composer's exception block (`In X line N:` and the message), or of Symfony's own error, without the
    command's usage."""
    lines = stderr.splitlines()
    start = next((i for i, l in enumerate(lines) if re.match(r'^In \S+ line \d+:$', l)), None)
    if start is None:
        if re.search(r'\n\n(?:outdated|show) \[', stderr):
            return [l.strip() for l in stderr.split('\n\n')[0].splitlines() if l.strip()]
        return None
    message = []
    for line in lines[start + 1:]:
        if re.match(r'^(?:outdated|show) \[', line) or line.startswith('Exception trace:'):
            break
        text = line[2:].rstrip() if line.startswith('  ') else line.rstrip()
        if re.match(r'^In \S+ line \d+:$', text):
            continue
        if text and not re.fullmatch(r'\[[\w\\]+\]', text):
            message.append(text)
    return message


def record(name, files, args, extra_env=None):
    home = tempfile.mkdtemp(prefix='pun-outdated-home-')
    work = tempfile.mkdtemp(prefix='pun-outdated-')
    try:
        for path, content in files.items():
            file = os.path.join(work, path)
            os.makedirs(os.path.dirname(file), exist_ok=True)
            with open(file, 'w') as f:
                f.write(content)
        r = composer(work, args, home, extra_env)
        here = os.path.realpath(work)
        case = {'name': name, 'files': files, 'args': args, 'exit': r.returncode,
                'stdout': r.stdout.replace(here, '{dir}').replace(work, '{dir}'),
                'stderr': r.stderr.replace(here, '{dir}').replace(work, '{dir}')}
        if extra_env:
            case['env'] = extra_env
        message = exception_message(case['stderr'])
        if 'Composer could not find' in case['stderr']:
            # Composer prints this error itself, outside an exception block; it is still a run error.
            message = [line for line in case['stderr'].splitlines() if line]
        if message is not None and r.returncode != 0:
            # The test reads only these lines of a run error.
            case['message'] = message
            case['stderr'] = ''
        assert '/var/folders' not in json.dumps(case) and home not in json.dumps(case), (name, case)
        return case
    finally:
        shutil.rmtree(home)
        shutil.rmtree(work)


def probe_script():
    source = open(PLATFORM_RS).read()
    return re.search(r'const PROBE: &str = r#"(.*?)"#;', source, re.S).group(1)


def main():
    version = subprocess.run(['php', PHAR, '--version'], capture_output=True, text=True).stdout
    assert 'Composer version 2.11.2 ' in version, version
    probed = subprocess.run(['php', '-d', 'display_errors=stderr', '-r', probe_script()], capture_output=True, text=True, check=True)
    probe = json.loads(probed.stdout)
    probe['ini'] = []
    probe['inputs'] = []
    cases = [record(*c) for c in CASES]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, 'w') as f:
        json.dump({'probe': probe, 'cases': cases}, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'{OUT}: PHP {probe["php"]}, {len(cases)} cases')


if __name__ == '__main__':
    main()
