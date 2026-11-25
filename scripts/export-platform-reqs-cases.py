#!/usr/bin/env python3
"""Write crates/pun-cli/tests/fixtures/check_platform_reqs.json, the manifest of crates/pun-cli/tests/check_platform_reqs.rs:
small projects with what `composer check-platform-reqs` 2.11.2 (.oracle/composer, the Composer the plugin host pins)
prints and returns for them, and the platform it checked them against.

Composer checks the real platform of the PHP it runs on. So the manifest records that platform once, as Pun's own probe
(the PROBE script of crates/pun-packages/src/resolver/platform.rs) run on the same `php`, without its `ini` and
`inputs` (local paths Pun only needs for hints and its cache). The test gives Pun that probe through PHP_BINARY, a
script that prints it, so it checks the same platform on any machine.

Each case is a set of files and the arguments after `check-platform-reqs`, run in a fresh temporary directory with a
fresh HOME and COMPOSER_HOME, COMPOSER_ROOT_VERSION=1.0.0 and COLUMNS=500, as scripts/export-validate-cases.py does. A
case records `exit`, `stdout` and `stderr`, and `message` (the lines of Composer's exception block) when Composer stops
with an exception, which Pun prints as its own run error (exit 2).

usage: scripts/export-platform-reqs-cases.py   (needs php 8.5 and .oracle/composer from scripts/oracle.sh)
"""
import json, os, re, shutil, subprocess, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PHAR = os.path.join(ROOT, '.oracle/composer/composer.phar')
OUT = os.path.join(ROOT, 'crates/pun-cli/tests/fixtures/check_platform_reqs.json')
PLATFORM_RS = os.path.join(ROOT, 'crates/pun-packages/src/resolver/platform.rs')


def j(value):
    return json.dumps(value, indent=4) + '\n'


ROOT_JSON = {'name': 'acme/app', 'require': {'php': '>=8.1'}}


def root(**fields):
    m = dict(ROOT_JSON)
    for k, v in fields.items():
        m[k.replace('_', '-')] = v
    return m


def package(name, **fields):
    p = {'name': name, 'version': '1.0.0', 'version_normalized': '1.0.0.0', 'type': 'library'}
    p.update({k.replace('_', '-'): v for k, v in fields.items()})
    return p


def installed(*packages, dev=(), vendor='vendor'):
    """vendor/composer/installed.json and each package's directory: Composer drops a package whose install path isn't
    there (`Factory::purgePackages`)."""
    files = {f'{vendor}/composer/installed.json': j({'packages': list(packages), 'dev': True, 'dev-package-names': list(dev)})}
    for p in packages:
        if p.get('type') != 'metapackage':
            files[f'{vendor}/{p["name"]}/.keep'] = ''
    return files


def lock(packages=(), packages_dev=()):
    data = {'content-hash': 'x', 'packages': list(packages)}
    if packages_dev is not None:
        data['packages-dev'] = list(packages_dev)
    return j(data)


OK = package('acme/ok', require={'php': '>=8.0', 'ext-json': '*', 'ext-mbstring': '*', 'lib-pcre': '>=8', 'acme/other': '^1.0'})
API = package('acme/api', require={'ext-ctype': '*', 'composer-plugin-api': '^2.0', 'composer-runtime-api': '^2.2', 'composer': '^2.0', 'php-64bit': '*', 'php-ipv6': '*'})
OLD = package('acme/old', require={'php': '<8.0'})
MISSING = package('acme/missing', require={'ext-nope': '*', 'lib-nope': '^1.0', 'php-zts': '*', 'php-debug': '*'})
POLYFILL = package('acme/polyfill', provide={'ext-fake': '1.2.0'})
WANTS_FAKE = package('acme/wants-fake', require={'ext-fake': '^1.0'})
WANTS_FAKE2 = package('acme/wants-fake2', require={'ext-fake': '^2.0'})
REPLACER = package('acme/replacer', replace={'ext-other-fake': '3.0.0'})
WANTS_OTHER = package('acme/wants-other', require={'ext-other-fake': '^3.0'})
MBSTRING_POLYFILL = package('acme/mbstring-polyfill', provide={'ext-mbstring': '*'})
DEV = package('acme/dev', require={'ext-nope': '*'})

CASES = [
    ('vendor-success', {'composer.json': j(root()), **installed(OK, API)}, []),
    ('vendor-failed', {'composer.json': j(root()), **installed(OLD)}, []),
    ('vendor-missing', {'composer.json': j(root()), **installed(MISSING)}, []),
    ('vendor-all', {'composer.json': j(root()), **installed(OK, OLD, MISSING)}, []),
    ('provided', {'composer.json': j(root()), **installed(POLYFILL, WANTS_FAKE)}, []),
    ('provided-failed', {'composer.json': j(root()), **installed(POLYFILL, WANTS_FAKE, WANTS_FAKE2)}, []),
    ('replaced', {'composer.json': j(root()), **installed(REPLACER, WANTS_OTHER)}, []),
    ('provided-and-real', {'composer.json': j(root()), **installed(MBSTRING_POLYFILL, OK)}, []),
    ('root-only', {'composer.json': j(root(require={'php': '>=8.1', 'ext-json': '*'}, require_dev={'ext-nope': '*'})), **installed(package('acme/plain'))}, []),
    ('root-only-no-dev', {'composer.json': j(root(require={'php': '>=8.1', 'ext-json': '*'}, require_dev={'ext-nope': '*'})), **installed(package('acme/plain'))}, ['--no-dev']),
    ('dev-packages', {'composer.json': j(root()), **installed(OK, DEV, dev=['acme/dev'])}, []),
    ('dev-packages-no-dev', {'composer.json': j(root()), **installed(OK, DEV, dev=['acme/dev'])}, ['--no-dev']),
    ('names-and-case', {'composer.json': j(root(require={'php': '>=8.1', 'ext-PDO': '*', 'ext-zend-opcache': '*', 'ext-Zend-OPcache': '*'})), **installed(package('acme/plain'))}, []),
    ('config-platform-ignored', {'composer.json': j(root(config={'platform': {'php': '7.0.0', 'ext-nope': '1.0'}})), **installed(OK, MISSING)}, []),
    ('vendor-dir-config', {'composer.json': j(root(config={'vendor-dir': 'lib'})), **installed(OLD, vendor='lib')}, []),
    # The lock: with --lock, or when vendor/ has nothing installed.
    ('lock', {'composer.json': j(root()), 'composer.lock': lock([OLD], [DEV]), **installed(OK)}, ['--lock']),
    ('lock-no-dev', {'composer.json': j(root()), 'composer.lock': lock([OLD], [DEV])}, ['--lock', '--no-dev']),
    ('no-vendor', {'composer.json': j(root()), 'composer.lock': lock([OK], [DEV])}, []),
    ('no-vendor-no-dev', {'composer.json': j(root()), 'composer.lock': lock([OK], [DEV])}, ['--no-dev']),
    ('empty-vendor', {'composer.json': j(root()), 'composer.lock': lock([OLD]), **installed()}, []),
    ('no-vendor-no-lock', {'composer.json': j(root())}, []),
    ('lock-without-packages-dev', {'composer.json': j(root()), 'composer.lock': lock([OK], None)}, ['--lock']),
    ('lock-without-packages-dev-no-dev', {'composer.json': j(root()), 'composer.lock': lock([OK], None)}, ['--lock', '--no-dev']),
    # --format=json.
    ('json-success', {'composer.json': j(root()), **installed(OK, API)}, ['--format=json']),
    ('json-all', {'composer.json': j(root()), **installed(OK, OLD, MISSING, POLYFILL, WANTS_FAKE, WANTS_FAKE2)}, ['--format', 'json']),
    ('json-short', {'composer.json': j(root()), **installed(OLD)}, ['-f', 'json']),
    ('json-empty', {'composer.json': j({'name': 'acme/app'}), **installed(package('acme/plain'))}, ['--format=json']),
    ('format-text', {'composer.json': j(root()), **installed(OLD)}, ['--format=text']),
    ('format-invalid', {'composer.json': j(root()), **installed(OLD)}, ['--format=xml']),
    # Branch aliases: the local repository and the lock hold an aliased package and its alias.
    ('alias-provider', {'composer.json': j(root()), **installed(package('acme/poly', version='dev-main', version_normalized='dev-main', extra={'branch-alias': {'dev-main': '2.x-dev'}}, provide={'ext-fake': 'self.version'}), package('acme/wants', require={'ext-fake': '^9.0'}))}, []),
    ('alias-failed-twice', {'composer.json': j(root()), **installed(package('acme/old-dev', version='dev-main', version_normalized='dev-main', default_branch=True, require={'php': '<8.0'}))}, []),
    ('two-providers-fail', {'composer.json': j(root()), **installed(package('acme/p1', provide={'ext-fake': '1.0.0'}), package('acme/p2', replace={'ext-fake': '1.5.0'}), WANTS_FAKE2)}, []),
    ('root-provides', {'composer.json': j(root(provide={'ext-fake': '2.1.0'}, require={'php': '>=8.1', 'ext-fake': '^2.0'})), **installed(package('acme/plain'))}, []),
    ('lock-root-alias', {'composer.json': j(root()), 'composer.lock': j({'content-hash': 'x', 'packages': [package('acme/lib', version='dev-main', version_normalized='dev-main', require={'php': '<8.0'})], 'packages-dev': [], 'aliases': [{'package': 'acme/lib', 'version': 'dev-main', 'alias': '1.0.0', 'alias_normalized': '1.0.0.0'}]})}, ['--lock']),
    ('working-dir', {'sub/composer.json': j(root()), **{'sub/' + k: v for k, v in installed(OLD).items()}}, ['-d', 'sub']),
    ('text-empty', {'composer.json': j({'name': 'acme/app'}), **installed(package('acme/plain'))}, []),
    ('no-dev-lock-fallback-dev-names', {'composer.json': j(root()), 'composer.lock': lock([OK], [DEV]), 'vendor/composer/installed.json': j({'packages': [OK], 'dev': True, 'dev-package-names': ['acme/ok']})}, ['--no-dev']),
    # Files that don't read as Composer expects them.
    ('installed-invalid-json', {'composer.json': j(root()), 'vendor/composer/installed.json': '{"packages": [\n', 'vendor/acme/old/.keep': '', 'composer.lock': lock([OLD], [])}, []),
    ('installed-list', {'composer.json': j(root()), 'vendor/composer/installed.json': j([OLD]), 'vendor/acme/old/.keep': ''}, []),
    ('installed-not-a-list', {'composer.json': j(root()), 'vendor/composer/installed.json': j({'packages': 5}), 'composer.lock': lock([OLD], [])}, []),
    ('lock-invalid-json', {'composer.json': j(root()), 'composer.lock': '{"packages": [\n'}, ['--lock']),
    # What lock-not-an-object gives without Composer's crash: a lock with no packages.
    ('lock-empty', {'composer.json': j(root()), 'composer.lock': lock([], [])}, ['--lock']),
    ('lock-not-an-object', {'composer.json': j(root()), 'composer.lock': '[]\n'}, ['--lock']),
    ('composer-json-array', {'composer.json': '[]\n'}, []),
    ('composer-json-schema', {'composer.json': j({'name': 'acme/app', 'require': {'php': 5}})}, []),
    ('no-composer-json', {}, []),
    # COMPOSER names the composer file, and its lock follows it.
    ('composer-env', {'custom.json': j(root()), 'custom.lock': lock([OLD], []), 'composer.json': j({'name': 'acme/other'})}, ['--lock'], {'COMPOSER': 'custom.json'}),
    ('composer-env-missing', {'composer.json': j(root())}, [], {'COMPOSER': 'nope.json'}),
    ('invalid-json', {'composer.json': '{"name": "acme/app",}\n'}, []),
    ('working-dir-missing', {'composer.json': j(root())}, ['-d', 'nope']),
    # A package whose directory is gone isn't installed, nor checked.
    ('package-directory-missing', {'composer.json': j(root()), **{k: v for k, v in installed(OK, OLD).items() if k != 'vendor/acme/old/.keep'}}, []),
    ('metapackage', {'composer.json': j(root()), **installed(package('acme/meta', type='metapackage', require={'ext-nope': '*'}))}, []),
]


def probe_script():
    source = open(PLATFORM_RS).read()
    return re.search(r'const PROBE: &str = r#"(.*?)"#;', source, re.S).group(1)


def composer(cwd, args, home, extra_env=None):
    env = {'PATH': os.environ['PATH'], 'HOME': home, 'COMPOSER_HOME': os.path.join(home, '.composer'),
           'COMPOSER_CACHE_DIR': os.path.join(home, '.cache'), 'COMPOSER_ROOT_VERSION': '1.0.0', 'COLUMNS': '500',
           **(extra_env or {})}
    return subprocess.run(['php', PHAR, '--no-interaction', *args], cwd=cwd, env=env, capture_output=True, text=True)


def write(dir, files):
    for path, content in files.items():
        full = os.path.join(dir, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, 'w') as f:
            f.write(content)


def exception_message(stderr):
    """The lines of Symfony's exception block ("In <file> line <n>:", then the padded message), or None."""
    lines = stderr.split('\n')
    start = next((i for i, l in enumerate(lines) if re.match(r'^In \S+ line \d+:$', l)), None)
    if start is None:
        return None
    message = []
    for line in lines[start + 1:]:
        if line.startswith('check-platform-reqs [') or line.startswith('Exception trace:'):
            break
        text = line[2:].rstrip() if line.startswith('  ') else line.rstrip()
        if re.match(r'^In \S+ line \d+:$', text):
            continue
        if text and not re.fullmatch(r'\[[\w\\]+\]', text):
            message.append(text)
    return message


def record(name, files, args, extra_env=None):
    home = tempfile.mkdtemp(prefix='pun-platform-home-')
    work = tempfile.mkdtemp(prefix='pun-platform-')
    try:
        write(work, files)
        r = composer(work, ['check-platform-reqs', *args], home, extra_env)
        stderr = r.stderr.replace(os.path.realpath(work), '{dir}')
        case = {'name': name, 'files': files, 'args': args, 'exit': r.returncode, 'stdout': r.stdout, 'stderr': stderr}
        if extra_env:
            case['env'] = extra_env
        message = exception_message(stderr)
        if 'Composer could not find' in stderr:
            # Composer prints this error itself, outside an exception block; it is still a run error.
            message = [line for line in stderr.splitlines() if line]
        if message is not None:
            # The test reads only these lines of a run error; the rest (a LogicException's trace) names local paths.
            case['message'] = message
            case['stderr'] = ''
        return case
    finally:
        shutil.rmtree(home)
        shutil.rmtree(work)


def main():
    version = subprocess.run(['php', PHAR, '--version'], capture_output=True, text=True).stdout
    assert 'Composer version 2.11.2 ' in version, version
    probed = subprocess.run(['php', '-d', 'display_errors=stderr', '-r', probe_script()], capture_output=True, text=True, check=True)
    probe = json.loads(probed.stdout)
    probe['ini'] = []
    probe['inputs'] = []
    cases = [record(*case) for case in CASES]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, 'w') as f:
        json.dump({'probe': probe, 'cases': cases}, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'{OUT}: PHP {probe["php"]}, {len(cases)} cases')


if __name__ == '__main__':
    main()
