# Independent recomputation of the paired summary from raw pass files.
import json, math, sys, os
d = sys.argv[1]
S = json.load(open(os.path.join(d, 'summary.json')))
def med(v):
    s = sorted(v); n = len(s); m = n // 2
    return s[m] if n % 2 else (s[m-1] + s[m]) / 2
def nr(v, f):
    s = sorted(v); return s[math.ceil(len(s) * f) - 1]
runs = {(i, p): json.load(open(os.path.join(d, f'{i}-pass-{p+1}.json'))) for i in ('rust', 'typescript') for p in (0, 1)}
ok = True
for M in S['modes']:
    mode = M['mode']
    g = lambda i, p: next(x for x in runs[(i, p)]['modes'] if x['mode'] == mode)
    corpus = {i: med(g(i, 0)['roundTotalMs'] + g(i, 1)['roundTotalMs']) for i in ('rust', 'typescript')}
    n = len(g('rust', 0)['perCase'])
    pm = {i: [med(g(i, 0)['perCase'][k]['ms'] + g(i, 1)['perCase'][k]['ms']) for k in range(n)] for i in ('rust', 'typescript')}
    pp = {(i, p): [med(g(i, p)['perCase'][k]['ms']) for k in range(n)] for i in ('rust', 'typescript') for p in (0, 1)}
    sp = [pm['typescript'][k] / pm['rust'][k] for k in range(n)]
    anyp = [g('rust', 0)['perCase'][k]['id'] for k in range(n) if any(pp[('typescript', p)][k] / pp[('rust', p)][k] < 1 for p in (0, 1))]
    # extraction-only = median of per-round sums of per-case timers over measured rounds
    ex = med([sum(r['caseMs']) for p in (0, 1) for r in g('rust', p)['rawRounds'] if r['phase'] == 'measured'])
    checks = {
        'speedup': abs(corpus['typescript'] / corpus['rust'] - M['speedup']) < 1e-12,
        'pageMedianSpeedup': abs(med(sp) - M['pageMedianSpeedup']) < 1e-12,
        'pageP95': abs(nr(pm['rust'], 0.95) - M['rust']['pageP95Ms']) < 1e-12,
        'anyPass': anyp == [c['id'] for c in M['rustSlowerInAnyPass']],
        'extractionOnly': abs(ex - M['rust']['extractionOnlyCorpusMedianMs']) < 1e-9,
        'noSmallP95': all('p95Ms' not in c for c in M['rust']['perCase']) and 'corpusP95Ms' not in M['rust'],
        'sampleCount': M['rust']['corpusSampleCount'] == 2 * S['rounds'],
    }
    ok &= all(checks.values()); print(mode, checks)
C = json.load(open(os.path.join(d, 'cold.json')))
cold_p95_keys = [k for m in C['modes'] for i in ('rust', 'typescript') for k in m[i] if 'p95' in k.lower()]
ok &= len(C['samples']) == 4 * S['rounds'] and not cold_p95_keys
print('cold samples', len(C['samples']), 'expected', 4 * S['rounds'], 'p95 keys', cold_p95_keys)
print('ALL_MATCH' if ok else 'MISMATCH')

if not ok:
    raise SystemExit(1)
