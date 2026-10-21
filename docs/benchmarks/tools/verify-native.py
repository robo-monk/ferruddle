# Independent recomputation of native-repeat statistics from raw run files.
import json, math, sys, os
d = sys.argv[1]
S = json.load(open(os.path.join(d, 'native-repeat-summary.json')))
raw = {(n, p): json.load(open(os.path.join(d, f'native-repeat-{n}-{p+1}.json'))) for n in ('baseline', 'current') for p in (0, 1)}
def med(v):
    s = sorted(v); n = len(s); m = n // 2
    return s[m] if n % 2 else (s[m-1] + s[m]) / 2
def nr(v, f):
    s = sorted(v); return s[math.ceil(len(s) * f) - 1]
ok = True
for M in S['modes']:
    mode = M['mode']
    get = lambda n, p: next(x for x in raw[(n, p)]['modes'] if x['mode'] == mode)
    ids = [c['id'] for c in get('current', 0)['perCase']]
    pm = {(n, p): [med(c['ms']) for c in get(n, p)['perCase']] for n in ('baseline', 'current') for p in (0, 1)}
    pooled = {n: [med(get(n, 0)['perCase'][i]['ms'] + get(n, 1)['perCase'][i]['ms']) for i in range(len(ids))] for n in ('baseline', 'current')}
    corpus = {n: med(get(n, 0)['roundTotalMs'] + get(n, 1)['roundTotalMs']) for n in ('baseline', 'current')}
    aa = [max(r, 1/r) for r in (pm[('current', 1)][i] / pm[('current', 0)][i] for i in range(len(ids)))]
    p95 = nr(aa, 0.95); thr = max(1.05, p95)
    pr = [[pm[('current', p)][i] / pm[('baseline', p)][i] for p in (0, 1)] for i in range(len(ids))]
    flags5 = [ids[i] for i in range(len(ids)) if all(r > 1.05 for r in pr[i])]
    flagsN = [ids[i] for i in range(len(ids)) if all(r > thr for r in pr[i])]
    slower = [ids[i] for i in range(len(ids)) if pooled['current'][i] / pooled['baseline'][i] > 1]
    changed = [ids[i] for i in range(len(ids)) if get('current', 0)['perCase'][i]['output'] != get('baseline', 0)['perCase'][i]['output']]
    checks = {
        'corpusRatio': abs(corpus['current'] / corpus['baseline'] - M['currentToBaselineRatio']) < 1e-12,
        'aaP95': abs(p95 - M['aaNoise']['p95FoldDifference']) < 1e-12,
        'threshold': abs(thr - M['regressionFlagThreshold']) < 1e-12,
        'flags5': flags5 == [c['id'] for c in M['slowerOver5PercentInBothPasses']],
        'flagsNoise': flagsN == [c['id'] for c in M['slowerBeyondNoiseFloorInBothPasses']],
        'slower': slower == [c['id'] for c in M['slowerCases']],
        'changed': changed == M['changedOutputProofs'],
        'passRatios': all(abs(a - b) < 1e-12 for i, c in enumerate(M['perCase']) for a, b in zip(c['passRatios'], pr[i])),
        'pooledRatio': all(abs(c['currentToBaselineRatio'] - pooled['current'][i] / pooled['baseline'][i]) < 1e-12 for i, c in enumerate(M['perCase'])),
    }
    ok &= all(checks.values())
    print(mode, checks, 'cases', len(ids))
print('ALL_MATCH' if ok else 'MISMATCH')

if not ok:
    raise SystemExit(1)
