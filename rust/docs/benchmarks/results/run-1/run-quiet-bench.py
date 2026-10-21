import collections, datetime, hashlib, json, os, pathlib, re, shutil, statistics, subprocess, sys
root=pathlib.Path.cwd(); cache=pathlib.Path('/home/robomonk/.cache/defuddle-port-hardening')
output=pathlib.Path(sys.argv[1]).resolve(); initial=json.loads((cache/'final-build.json').read_text())
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
now=lambda:datetime.datetime.now(datetime.timezone.utc).isoformat()
def state():
    vm={line.split()[0]:int(line.split()[1]) for line in pathlib.Path('/proc/vmstat').read_text().splitlines()}
    mem={line.split()[0].rstrip(':'):int(line.split()[1]) for line in pathlib.Path('/proc/meminfo').read_text().splitlines()}
    ps=subprocess.check_output(['ps','-eo','stat=,wchan=,comm='],text=True)
    blocked=collections.Counter(line.split()[1] for line in ps.splitlines() if line.split()[0].startswith('D'))
    return {'at':now(),'loadAverage':os.getloadavg(),'logicalCpus':os.cpu_count(),
        'cpuJiffies':[int(x) for x in pathlib.Path('/proc/stat').read_text().splitlines()[0].split()[1:]],
        'swapPagesIn':vm['pswpin'],'swapPagesOut':vm['pswpout'],
        'memoryKiB':{k:mem[k] for k in ['MemTotal','MemFree','MemAvailable','SwapTotal','SwapFree']},
        'blockedTasksByWaitChannel':dict(blocked)}
def validate_frozen():
    actual={str(p.relative_to(root)):sha(p) for p in sorted((root/'rust/src').rglob('*.rs'))}
    if actual != initial['runtimeSourceHashes']:raise RuntimeError('Runtime changed after final prebuild')
    for name in ['current','baseline']:
        if sha(initial[name+'Executable']) != initial[name+'ExecutableSha256']:raise RuntimeError(name+' binary changed')
validate_frozen()
if output.exists() and any(output.iterdir()):raise RuntimeError('Final evidence directory must be new and empty')
monitor_path=cache/'final-vmstat.log'; log_path=cache/'final-benchmark.log'
before=state(); env={**os.environ,'TZ':'UTC'}
preidle=subprocess.check_output(['vmstat','-w','-t','1','3'],env=env,text=True)
(cache/'final-preidle.log').write_text(preidle)
monitor=None
try:
    with monitor_path.open('w') as mf,log_path.open('w') as lf:
        monitor=subprocess.Popen(['stdbuf','-oL','vmstat','-w','-t','5'],stdout=mf,stderr=subprocess.STDOUT,env=env)
        print('Starting final paired benchmark; progress in '+str(log_path),flush=True)
        subprocess.run(['node','scripts/porting/benchmark.mjs',str(output),'7','2'],cwd=root,env=env,stdout=lf,stderr=subprocess.STDOUT,check=True)
        print('Paired benchmark passed; starting immediate native repeat.',flush=True)
        subprocess.run(['node','scripts/porting/benchmark-native-repeat.mjs',str(output),initial['baselineExecutable'],str(cache/'benchmark-baseline-worker')],cwd=root,env=env,stdout=lf,stderr=subprocess.STDOUT,check=True)
        if monitor.poll() is not None:raise RuntimeError('vmstat monitor stopped unexpectedly')
finally:
    if monitor is not None:
        monitor.terminate()
        try:monitor.wait(timeout=5)
        except subprocess.TimeoutExpired:monitor.kill();monitor.wait()
after=state();validate_frozen()
summary=json.loads((output/'summary.json').read_text());repeat=json.loads((output/'native-repeat-summary.json').read_text())
actual={p:h for p,h in summary['rustSourceHashes'].items() if p.startswith('rust/src/')}
assert actual==initial['runtimeSourceHashes']
assert summary['executableSha256']==initial['currentExecutableSha256']
assert repeat['binaryHashes']['baseline']==initial['baselineExecutableSha256']
rows=[]; names=None
for line in monitor_path.read_text().splitlines():
    tokens=line.split()
    if tokens[:2]==['r','b']:
        names=tokens[:18] if 'gu' in tokens else tokens[:17]
    elif names and len(tokens)>=len(names) and all(re.fullmatch(r'\d+',t) for t in tokens[:len(names)]):
        rows.append({**dict(zip(names,map(int,tokens[:len(names)]))),'timestamp':' '.join(tokens[len(names):])})
# vmstat's first sample is the average since boot, rather than this window.
samples=rows[1:]
assert samples,'No interval vmstat samples'
metrics={'sampleIntervalSeconds':5,'samples':len(samples),'firstBootAverageExcluded':True,
    'idlePercentMedian':statistics.median(r['id'] for r in samples),
    'idlePercentMin':min(r['id'] for r in samples),
    'userPlusSystemPercentMedian':statistics.median(r['us']+r['sy'] for r in samples),
    'userPlusSystemPercentMax':max(r['us']+r['sy'] for r in samples),
    'swapInOutNonzeroSamples':sum(r['si']!=0 or r['so']!=0 for r in samples),
    'swapPagesInDelta':after['swapPagesIn']-before['swapPagesIn'],
    'swapPagesOutDelta':after['swapPagesOut']-before['swapPagesOut'],
    'runnableMax':max(r['r'] for r in samples),'ioWaitPercentMax':max(r['wa'] for r in samples)}
shutil.copyfile(monitor_path,output/'vmstat.log');(output/'preidle.log').write_text(preidle)
shutil.copyfile(__file__,output/'run-quiet-bench.py')
record={'before':before,'after':after,'monitor':metrics,'vmstatLogSha256':sha(monitor_path),
    'runnerSha256':sha(__file__),'prebuiltArtifacts':initial,
    'runtimeAndBinaryHashesUnchanged':True,'allCommandsExitedZero':True,
    'notes':'No other task agents/builds/tests active. Load average includes preexisting GPU-driver waits; assess CPU idle and swap IO, not load average alone. Interval CPU activity also includes the benchmark workers import/validation phases.'}
(output/'quiet-window.json').write_text(json.dumps(record,indent=2)+'\n')
print('Both final benchmarks passed; source and executable identities match refreeze.',flush=True)
print(json.dumps(metrics,indent=2),flush=True)
