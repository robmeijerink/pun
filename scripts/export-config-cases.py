#!/usr/bin/env python3
"""Write crates/pun-cli/tests/fixtures/config.json, the manifest of crates/pun-cli/tests/config.rs: small projects with
what `composer config` 2.11.2 (.oracle/composer, the Composer the plugin host pins) prints and returns for them, and the
files it leaves behind.

Each case is a set of files in the project (`files`) and in COMPOSER_HOME (`home_files`), and the words after
`composer`, run in a fresh temporary directory with a fresh HOME and COMPOSER_HOME, COMPOSER_ROOT_VERSION=1.0.0 and
COLUMNS=500 (unless the case's `env` says otherwise; a null leaves a variable out), without a terminal on stdin. The
project's directory is written `{dir}` and COMPOSER_HOME `{home}` in the output and the files. A case records `exit`,
`stdout` and `stderr`, `after` and `home_after` (every file of the project and of COMPOSER_HOME afterwards, byte for
byte), and `message` (the lines of Composer's exception block) when Composer stops with an exception, which Pun prints
as its own run error (exit 2).

usage: scripts/export-config-cases.py   (needs php 8.5 and .oracle/composer from scripts/oracle.sh)
"""
import json, os, re, shutil, subprocess, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PHAR = os.path.join(ROOT, '.oracle/composer/composer.phar')
OUT = os.path.join(ROOT, 'crates/pun-cli/tests/fixtures/config.json')


def j(value, indent=4):
    return json.dumps(value, indent=indent) + '\n'


ROOT_JSON = {'name': 'acme/app', 'description': 'The application', 'require': {'php': '>=8.1'},
             'config': {'sort-packages': True, 'vendor-dir': 'lib/vendor', 'allow-plugins': {'acme/plugin': True}},
             'extra': {'laravel': {'dont-discover': []}}}
PROJECT = {'composer.json': j(ROOT_JSON)}
TWO_SPACES = {'composer.json': j(ROOT_JSON, indent=2)}
TABS = {'composer.json': json.dumps(ROOT_JSON, indent='\t') + '\n'}
NO_NEWLINE = {'composer.json': json.dumps({'name': 'acme/app'}, indent=4)}
BARE = {'composer.json': '{}\n'}
WITH_REPOS = {'composer.json': j({'name': 'acme/app', 'repositories': [{'type': 'vcs', 'url': 'https://example.org/one.git'}]})}
GLOBAL = {'config.json': j({'config': {'process-timeout': 900, 'github-protocols': ['https']}})}

CASES = [
    # Reading a setting: the project's, else the global one, else the default.
    ('get-default', PROJECT, ['config', 'process-timeout']),
    ('get-project', PROJECT, ['config', 'vendor-dir']),
    ('get-absolute', PROJECT, ['config', 'vendor-dir', '--absolute']),
    ('get-bin-dir', PROJECT, ['config', 'bin-dir']),
    ('get-global', PROJECT, ['config', 'process-timeout'], {}, GLOBAL),
    ('get-global-option', PROJECT, ['config', '--global', 'process-timeout'], {}, GLOBAL),
    ('get-boolean', PROJECT, ['config', 'sort-packages']),
    ('get-array', PROJECT, ['config', 'github-protocols'], {}, GLOBAL),
    ('get-object', PROJECT, ['config', 'allow-plugins']),
    ('get-nested', PROJECT, ['config', 'allow-plugins.acme/plugin']),
    ('get-source', PROJECT, ['config', 'vendor-dir', '--source']),
    ('get-extra', PROJECT, ['config', 'extra']),
    ('get-extra-key', PROJECT, ['config', 'extra.laravel']),
    ('get-property', PROJECT, ['config', 'name']),
    ('get-repositories', WITH_REPOS, ['config', 'repositories']),
    ('get-undefined', PROJECT, ['config', 'nope']),
    ('get-undefined-extra', PROJECT, ['config', 'extra.nope']),
    ('get-env', PROJECT, ['config', 'process-timeout'], {'COMPOSER_PROCESS_TIMEOUT': '42'}),
    # The list of every setting.
    ('list', PROJECT, ['config', '--list']),
    ('list-source', PROJECT, ['config', '-l', '--source'], {}, GLOBAL),
    ('list-global', PROJECT, ['config', '--list', '--global'], {}, GLOBAL),
    # Setting one: config keys, with their validators and normalizers.
    ('set-integer', PROJECT, ['config', 'process-timeout', '600']),
    ('set-boolean', PROJECT, ['config', 'secure-http', 'false']),
    ('set-boolean-true', PROJECT, ['config', 'optimize-autoloader', 'true']),
    ('set-string', PROJECT, ['config', 'bin-dir', 'tools']),
    ('set-array', PROJECT, ['config', 'github-protocols', 'https', 'ssh']),
    ('set-allow-plugins', PROJECT, ['config', '--no-plugins', 'allow-plugins.acme/other', 'true']),
    ('set-allow-plugins-false', PROJECT, ['config', 'allow-plugins.acme/plugin', 'false']),
    ('set-preferred-install', PROJECT, ['config', 'preferred-install.acme/*', 'source']),
    ('set-platform', PROJECT, ['config', 'platform.php', '8.2.0']),
    ('set-platform-false', PROJECT, ['config', 'platform.ext-foo', 'false']),
    ('set-sort-packages-two-spaces', TWO_SPACES, ['config', 'process-timeout', '600']),
    ('set-tabs', TABS, ['config', 'process-timeout', '600']),
    ('set-no-newline', NO_NEWLINE, ['config', 'process-timeout', '600']),
    ('set-bare', BARE, ['config', 'process-timeout', '600']),
    ('set-global', PROJECT, ['config', '--global', 'process-timeout', '1200']),
    ('set-global-existing', PROJECT, ['config', '-g', 'secure-http', 'false'], {}, GLOBAL),
    ('set-file', {**PROJECT, 'other.json': j({'name': 'acme/other'})}, ['config', '--file', 'other.json', 'process-timeout', '7']),
    ('set-composer-env', {**PROJECT, 'custom.json': j({'name': 'acme/custom'})}, ['config', 'process-timeout', '8'], {'COMPOSER': 'custom.json'}),
    # Root properties.
    ('set-name', PROJECT, ['config', 'name', 'acme/renamed']),
    ('set-description', PROJECT, ['config', 'description', 'A new description']),
    ('set-minimum-stability', PROJECT, ['config', 'minimum-stability', 'dev']),
    ('set-prefer-stable', PROJECT, ['config', 'prefer-stable', 'true']),
    ('set-keywords', PROJECT, ['config', 'keywords', 'one', 'two']),
    ('set-license', PROJECT, ['config', 'license', 'MIT']),
    ('set-type', PROJECT, ['config', 'type', 'project']),
    ('unset-description', PROJECT, ['config', '--unset', 'description']),
    ('set-property-global', PROJECT, ['config', '--global', 'name', 'acme/x']),
    # extra.
    ('set-extra', PROJECT, ['config', 'extra.foo.bar', 'baz']),
    ('set-extra-json', PROJECT, ['config', '--json', 'extra.list', '[1, "two", {"three": 3}]']),
    ('set-extra-merge', PROJECT, ['config', '--json', '--merge', 'extra.laravel', '{"providers": ["App\\\\Provider"]}']),
    ('set-extra-merge-array', {'composer.json': j({'name': 'acme/app', 'extra': {'list': [1]}})}, ['config', '--json', '--merge', 'extra.list', '[2]']),
    ('set-extra-merge-mixed', {'composer.json': j({'name': 'acme/app', 'extra': {'list': [1]}})}, ['config', '--json', '--merge', 'extra.list', '{"a": 2}']),
    ('set-extra-invalid-json', PROJECT, ['config', '--json', 'extra.list', '[1,']),
    ('unset-extra', PROJECT, ['config', '--unset', 'extra.laravel']),
    # Repositories.
    ('set-repository', PROJECT, ['config', 'repositories.foo', 'vcs', 'https://example.org/foo.git']),
    ('set-repository-append', WITH_REPOS, ['config', '--append', 'repositories.foo', 'vcs', 'https://example.org/foo.git']),
    ('set-repository-prepend', WITH_REPOS, ['config', 'repositories.foo', 'composer', 'https://repo.example.org']),
    ('set-repository-json', PROJECT, ['config', 'repo.foo', '{"type": "path", "url": "../foo"}']),
    ('set-repository-packagist-off', PROJECT, ['config', 'repositories.packagist.org', 'false']),
    ('set-repository-one-value', PROJECT, ['config', 'repositories.foo', 'vcs']),
    ('unset-repository', WITH_REPOS, ['config', '--unset', 'repositories.0']),
    # Auth.
    ('set-github-oauth', PROJECT, ['config', 'github-oauth.github.com', 'ghp_token']),
    ('set-github-oauth-global', PROJECT, ['config', '-g', 'github-oauth.github.com', 'ghp_token']),
    ('set-http-basic', PROJECT, ['config', 'http-basic.repo.example.org', 'user', 'secret']),
    ('set-http-basic-one', PROJECT, ['config', 'http-basic.repo.example.org', 'user']),
    ('set-bearer', PROJECT, ['config', 'bearer.repo.example.org', 'token']),
    ('unset-github-oauth', {**PROJECT, 'auth.json': j({'github-oauth': {'github.com': 'ghp_token'}})}, ['config', '--unset', 'github-oauth.github.com']),
    # Unsetting and errors.
    ('unset', PROJECT, ['config', '--unset', 'vendor-dir']),
    ('unset-missing', PROJECT, ['config', '--unset', 'process-timeout']),
    ('unset-with-value', PROJECT, ['config', '--unset', 'process-timeout', '5']),
    ('set-invalid-integer', PROJECT, ['config', 'process-timeout', 'abc']),
    ('set-invalid-choice', PROJECT, ['config', 'preferred-install', 'sometimes']),
    ('set-too-many', PROJECT, ['config', 'process-timeout', '1', '2']),
    ('set-unknown', PROJECT, ['config', 'nope', 'value']),
    ('no-key', PROJECT, ['config']),
    ('no-composer-json', {}, ['config', 'process-timeout', '600']),
    ('no-composer-json-get', {}, ['config', 'process-timeout']),
]


def composer(cwd, args, home, extra_env=None):
    env = {'PATH': os.environ['PATH'], 'HOME': home, 'COMPOSER_HOME': os.path.join(home, '.composer'),
           'COMPOSER_CACHE_DIR': os.path.join(home, '.cache'), 'COMPOSER_ROOT_VERSION': '1.0.0', 'COLUMNS': '500',
           **(extra_env or {})}
    env = {k: v for k, v in env.items() if v is not None}
    return subprocess.run(['php', PHAR, *args], cwd=cwd, env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL,
                          timeout=60)


def exception_message(stderr):
    """The lines of Composer's exception block (`In X line N:` and the message), or of Symfony's own error, without the
    command's usage."""
    lines = stderr.splitlines()
    start = next((i for i, l in enumerate(lines) if re.match(r'^In \S+ line \d+:$', l)), None)
    if start is None:
        if re.search(r'\n\nconfig \[', stderr):
            return [l.strip() for l in stderr.split('\n\n')[0].splitlines() if l.strip()]
        return None
    message = []
    for line in lines[start + 1:]:
        if line.startswith('config [') or line.startswith('Exception trace:'):
            break
        text = line[2:].rstrip() if line.startswith('  ') else line.rstrip()
        if re.match(r'^In \S+ line \d+:$', text):
            continue
        if text and not re.fullmatch(r'\[[\w\\]+\]', text):
            message.append(text)
    return message


def tree(base, replacements):
    """Every file under `base` (but caches), by its path, with the temporary directories written as placeholders."""
    out = {}
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d != 'cache')
        for name in sorted(filenames):
            path = os.path.join(dirpath, name)
            text = open(path, encoding='utf-8').read()
            for old, new in replacements:
                text = text.replace(old, new)
            out[os.path.relpath(path, base)] = text
    return out


def record(name, files, args, extra_env=None, home_files=None):
    home = tempfile.mkdtemp(prefix='pun-config-home-')
    work = tempfile.mkdtemp(prefix='pun-config-')
    composer_home = os.path.join(home, '.composer')
    try:
        for base, contents in ((work, files), (composer_home, home_files or {})):
            for path, content in contents.items():
                file = os.path.join(base, path)
                os.makedirs(os.path.dirname(file), exist_ok=True)
                with open(file, 'w') as f:
                    f.write(content)
        r = composer(work, args, home, extra_env)
        replacements = [(os.path.realpath(composer_home), '{home}'), (composer_home, '{home}'),
                        (os.path.realpath(work), '{dir}'), (work, '{dir}'),
                        (os.path.realpath(home), '{user}'), (home, '{user}')]

        def placeholders(text):
            for old, new in replacements:
                text = text.replace(old, new)
            return text

        case = {'name': name, 'files': files, 'home_files': home_files or {}, 'args': args, 'exit': r.returncode,
                'stdout': placeholders(r.stdout), 'stderr': placeholders(r.stderr),
                'after': tree(work, replacements), 'home_after': tree(composer_home, replacements)}
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
        dumped = json.dumps(case)
        assert '/var/folders' not in dumped and home not in dumped, (name, [l for l in dumped.split('\\n') if '/var/folders' in l][:3])
        return case
    finally:
        shutil.rmtree(home)
        shutil.rmtree(work)


def main():
    version = subprocess.run(['php', PHAR, '--version'], capture_output=True, text=True).stdout
    assert 'Composer version 2.11.2 ' in version, version
    cases = [record(*c) for c in CASES]
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, 'w') as f:
        json.dump({'cases': cases}, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'{OUT}: {len(cases)} cases')


if __name__ == '__main__':
    main()
