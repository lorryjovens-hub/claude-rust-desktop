"""Opt-in, build-time translation seeding for public UI strings (never run by the app).

Usage: python scripts/seed-translations.py [locale-id ...]
Uses Google's public translation endpoint via curl, with per-language resumable
cache in the OS temporary directory. Review generated translations before release.
"""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
LOCALES = ROOT / 'src' / 'locales'
CACHE = Path(tempfile.gettempdir()) / 'claude-desktop-translation-seed'
CACHE.mkdir(exist_ok=True)
PROTECTED = re.compile(r'\{\w+\}|</?[^>]+>|\n+|\b(?:Claude Rust|Claude|Clawparrot|Anthropic|GitHub|Google Drive|Markdown|SKILL\.md|YAML|JSON|MCP|API|H5|IM|Cron|Ctrl|Shift|Alt|Enter|QQ|Pro|Max|Top P|Top K)\b|\.(?:md|zip|skill)\b')


def flatten(tree, prefix=''):
    result = {}
    for key, value in tree.items():
        name = f'{prefix}.{key}' if prefix else key
        if isinstance(value, dict):
            result.update(flatten(value, name))
        else:
            result[name] = value
    return result


def unflatten(flat):
    result = {}
    for key, value in flat.items():
        node = result
        parts = key.split('.')
        for part in parts[:-1]:
            node = node.setdefault(part, {})
        node[parts[-1]] = value
    return result


def protect(value):
    tokens = []
    def replace(match):
        tokens.append(match.group())
        return f'__{900000 + len(tokens) - 1}__'
    return PROTECTED.sub(replace, value), tokens


def translate_batch(items, source_language, target_language):
    encoded = {}
    for index, value in items:
        encoded[index] = protect(value)
    single = len(items) == 1
    query = next(iter(encoded.values()))[0] if single else '\n'.join(f'[{index:05}] {value}' for index, (value, _) in encoded.items())
    for attempt in range(4):
        try:
            result = subprocess.run([
                'curl', '--fail', '--silent', '--show-error', '--max-time', '45',
                '--get', 'https://translate.googleapis.com/translate_a/single',
                '--data-urlencode', 'client=gtx', '--data-urlencode', f'sl={source_language}',
                '--data-urlencode', f'tl={target_language}', '--data-urlencode', 'dt=t',
                '--data-urlencode', f'q={query}',
            ], check=True, capture_output=True, encoding='utf-8')
            response = json.loads(result.stdout)
            output = ''.join(part[0] or '' for part in response[0])
            if single:
                output = f'[{items[0][0]:05}] ' + output
            else:
                # Some languages omit reference-like numeric prefixes but retain
                # every input line. Only recover when both order and count agree.
                lines = output.strip().splitlines()
                if len(lines) == len(items):
                    recovered = []
                    for (index, _), line in zip(items, lines):
                        marker = re.match(r'^\s*\[\s*(\d{5})\s*\]\s*', line)
                        if marker and int(marker[1]) != index:
                            raise ValueError('Translation reordered segments')
                        recovered.append(f'[{index:05}] ' + (line[marker.end():] if marker else line))
                    output = '\n'.join(recovered)
            matches = list(re.finditer(r'\[\s*(\d{5})\s*\]', output))
            if [int(m[1]) for m in matches] != list(encoded):
                raise ValueError('Translation changed the segment markers')
            translated = {}
            for position, match in enumerate(matches):
                index = int(match[1])
                end = matches[position + 1].start() if position + 1 < len(matches) else len(output)
                value = output[match.end():end].strip()
                for token_index, token in enumerate(encoded[index][1]):
                    marker = f'__{900000 + token_index}__'
                    if value.count(marker) != 1:
                        raise ValueError(f'Translation changed a protected token in segment {index}')
                    value = value.replace(marker, token)
                if re.search(r'__9\d{5}__', value):
                    raise ValueError('Unresolved protected token')
                translated[index] = value
            return translated
        except (subprocess.SubprocessError, ValueError, KeyError, TypeError) as error:
            if isinstance(error, ValueError) or attempt == 3:
                # Isolate troublesome strings without discarding successful work.
                if len(items) > 1:
                    print(f'{target_language}: splitting batch to preserve markers/placeholders', flush=True)
                    midpoint = len(items) // 2
                    return {**translate_batch(items[:midpoint], source_language, target_language),
                            **translate_batch(items[midpoint:], source_language, target_language)}
                raise RuntimeError(f'{target_language}: failed segment {items[0][0]}: {error}') from error
            time.sleep(2 ** attempt)


def seed(locale):
    source_id = 'zh' if locale['id'] == 'zh-TW' else 'en'
    source = flatten(json.loads((LOCALES / f'{source_id}.json').read_text(encoding='utf-8')))
    target = 'zh-TW' if locale['id'] == 'zh-TW' else locale['locale'].split('-')[0]
    source_language = 'zh-CN' if source_id == 'zh' else 'en'
    digest = hashlib.sha256(json.dumps(source, ensure_ascii=False).encode()).hexdigest()[:12]
    cache_file = CACHE / f'{locale["id"]}-{digest}.json'
    cached = {}
    for previous in sorted(CACHE.glob(f'{locale["id"]}-*.json'), key=lambda path: path.stat().st_mtime):
        cached.update(json.loads(previous.read_text(encoding='utf-8')))
    cached = {key: value for key, value in cached.items() if value.strip()}
    for key in ('settings.chinese', 'settings.english'):
        cached[source[key]] = source[key]
    overrides = json.loads((ROOT / 'scripts' / 'translation-overrides.json').read_text(encoding='utf-8')).get(locale['id'], {})
    for key, value in overrides.items():
        cached[source[key]] = value
    unique = list(dict.fromkeys(source.values()))
    pending = [(index, value) for index, value in enumerate(unique) if value and value not in cached]
    batches, batch, length = [], [], 0
    for item in pending:
        size = len(protect(item[1])[0]) + 10
        if batch and length + size > 3200:
            batches.append(batch)
            batch, length = [], 0
        batch.append(item)
        length += size
    if batch:
        batches.append(batch)
    for number, batch in enumerate(batches):
        result = translate_batch(batch, source_language, target)
        cached.update({unique[index]: value for index, value in result.items()})
        cache_file.write_text(json.dumps(cached, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        print(f'{locale["id"]}: batch {number + 1}/{len(batches)}', flush=True)
    flat = {key: cached[value] if value else value for key, value in source.items()}
    flat.update(overrides)
    for key, original in source.items():
        if sorted(re.findall(r'\{\w+\}', original)) != sorted(re.findall(r'\{\w+\}', flat[key])):
            raise ValueError(f'{locale["id"]}.{key}: placeholder mismatch')
    (LOCALES / f'{locale["id"]}.json').write_text(json.dumps(unflatten(flat), ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f'{locale["id"]}: saved {len(flat)} translations', flush=True)


if __name__ == '__main__':
    languages = json.loads((LOCALES / 'languages.json').read_text(encoding='utf-8'))
    requested = set(sys.argv[1:])
    selected = [item for item in languages if item['id'] not in ('en', 'zh') and (not requested or item['id'] in requested)]
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        futures = [pool.submit(seed, locale) for locale in selected]
        for future in concurrent.futures.as_completed(futures):
            future.result()
