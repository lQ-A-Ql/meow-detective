"""Validate the public corpus and run bounded owning-crate suites without private fallbacks."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent

def run_suite(arguments, output, timeout):
    process = subprocess.Popen(['cargo', *arguments], cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
    try:
        return process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        if os.name == 'nt':
            subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15, check=False)
        else:
            process.kill()
        process.wait(timeout=15)
        raise

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def checked_path(relative):
    path = (ROOT/relative).resolve()
    if ROOT not in path.parents or not path.is_file():
        raise ValueError(f'Corpus source must be a repository file: {relative}')
    return path

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--group', choices=['small','corrupt','resource','integration','all'], default='small')
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--output', default='artifacts/corpus')
    args = parser.parse_args()
    manifest = json.loads((ROOT/'scripts/baselines/forensic-corpus.json').read_text(encoding='utf-8-sig'))
    seen = set()
    for sample in manifest['samples']:
        if sample['id'] in seen or sample['private'] or not sample['license'] or not sample['expected']:
            raise ValueError('Invalid public corpus identity or provenance')
        seen.add(sample['id'])
        source = checked_path(sample['path'])
        if source.stat().st_size > sample['maxBytes'] or digest(source) != sample['sha256']:
            raise ValueError(f'Corpus checksum/budget mismatch: {sample["id"]}')
    for generator in manifest['generators']:
        if digest(checked_path(generator['source'])) != generator['sourceSha256']:
            raise ValueError(f'Generator changed; review its contract: {generator["id"]}')
    directory = ROOT/args.output/(time.strftime('%Y%m%d-%H%M%S')+'-'+str(time.time_ns() % 1000000000))
    directory.mkdir(parents=True, exist_ok=False)
    report = dict(schemaVersion=1, status='running', samples=len(seen), group=args.group, suites=[],
        private=[dict(environment=key,status='NOT-RUN',reason='explicit private run required') for key in manifest['privateEnvironments']],
        fuzz=dict(status='NOT-RUN',reason='long-running sanitizer/fuzz not part of deterministic smoke'),
        referenceTools=[])
    for tool in ['fls','istat','img_cat','mmls']:
        executable = shutil.which(tool)
        if executable:
            version = subprocess.run([executable,'-V'], capture_output=True, timeout=10, check=False)
            report['referenceTools'].append(dict(name=tool,path=executable,sha256=digest(Path(executable)),
                version=(version.stdout+version.stderr).decode(errors='replace').strip()))
    try:
        if args.run:
            for suite in manifest['suites']:
                if args.group != 'all' and suite['group'] != args.group:
                    continue
                if any(not isinstance(arg,str) or not arg or len(arg)>128 for arg in suite['cargo']):
                    raise ValueError('Invalid fixed corpus command')
                log = directory/(suite['id']+'.log')
                with log.open('wb') as output:
                    code = run_suite(suite['cargo'], output, suite['timeoutSeconds'])
                report['suites'].append(dict(id=suite['id'],exitCode=code,log=str(log)))
                if code:
                    raise RuntimeError(f'Corpus suite failed: {suite["id"]}')
        report['status'] = 'passed' if args.run else 'manifest-verified; suites-not-run'
    except Exception as error:
        report['status'] = 'failed'
        report['error'] = str(error)
        raise
    finally:
        (directory/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps(dict(status=report['status'],report=str(directory/'report.json')),ensure_ascii=False))

if __name__ == '__main__':
    main()
