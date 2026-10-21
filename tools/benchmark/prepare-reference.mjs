// Reconstruct the exact measured TypeScript reference outside the Rust source tree.
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const output = resolve(process.argv[2] ?? join(root, '.benchmarks/reference'));
if (existsSync(output) && readdirSync(output).length) throw new Error(`Use a new empty directory: ${output}`);
const python = process.env.PYTHON ?? 'python3';
const prepare = String.raw`
import hashlib,json,sys,tarfile
from pathlib import Path,PurePosixPath
benchmark=Path(sys.argv[1]);output=Path(sys.argv[2])
manifest=json.loads((benchmark/'measured-source.json').read_text())
archive=benchmark/manifest['archive']
assert hashlib.sha256(archive.read_bytes()).hexdigest()==manifest['archiveSha256']
files={}
with tarfile.open(archive,'r:gz') as source:
    for member in source.getmembers():
        path=PurePosixPath(member.name)
        assert member.isfile() and not path.is_absolute() and '..' not in path.parts
        assert member.name not in files
        data=source.extractfile(member).read()
        assert hashlib.sha256(data).hexdigest()==manifest['files'][member.name],member.name
        files[member.name]=data
assert set(files)==set(manifest['files'])
output.mkdir(parents=True,exist_ok=True)
for name,data in files.items():
    if name=='node_modules/.package-lock.json':continue
    target=output/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
metadata={'archiveSha256':manifest['archiveSha256'],'files':{k:v for k,v in manifest['files'].items() if k!='node_modules/.package-lock.json'}}
(output/'ferruddle-reference.json').write_text(json.dumps(metadata,indent=2)+'\n')
`;
execFileSync(python, ['-c', prepare, join(root, 'docs/benchmarks'), output], { stdio: 'inherit' });
execFileSync('npm', ['ci', '--ignore-scripts'], { cwd: output, stdio: 'inherit' });
console.log(`Pinned TypeScript reference ready: ${output}`);
