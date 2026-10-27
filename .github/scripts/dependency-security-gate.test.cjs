'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { mustScanLocally, verdict, watchdogRuns, checkLatestAudit, MAX_AGE_MS } = require('./dependency-security-gate.cjs');

const now = Date.UTC(2026, 9, 22, 12);
const run = (conclusion = 'success', status = 'completed') => ({
  id: 41, event: 'schedule', head_branch: 'main', status, conclusion,
  updated_at: new Date(now - 1000).toISOString(),
});
const job = (conclusion = 'success', ageMs = 60000) => ({
  name: 'RustSec dependency audit', conclusion,
  completed_at: new Date(now - ageMs).toISOString(),
});
test('recent real RustSec success admits, not a skipped decision', () => {
  assert.equal(verdict(run(), [job()], now).kind, 'allow');
  assert.equal(verdict(run(), [job('skipped')], now).kind, 'skip');
});
test('watchdog failure never falls back to an older success', () => {
  assert.equal(verdict(run('failure'), [job('failure')], now).kind, 'deny');
  assert.equal(verdict(run('failure'), [job('skipped')], now).kind, 'deny');
});
test('missing, queued, stale and future dated audits are HOLD', () => {
  assert.equal(verdict(run(), [], now).kind, 'deny');
  assert.equal(verdict(run(null, 'in_progress'), [job(null)], now).kind, 'deny');
  assert.equal(verdict(run(), [job('success', MAX_AGE_MS + 1000)], now).kind, 'deny');
  assert.equal(verdict(run(), [job('success', -10 * 60 * 1000)], now).kind, 'deny');
});
test('only expected production dependencies and security policy require PR rescans', () => {
  assert.equal(mustScanLocally(['docs/README.md']), false);
  assert.equal(mustScanLocally(['Cargo.lock']), true);
  assert.equal(mustScanLocally(['iroh-base/Cargo.toml']), true);
  assert.equal(mustScanLocally(['.github/workflows/ci.yml']), true);
  assert.equal(mustScanLocally(['.github/scripts/dependency-security-gate.cjs']), true);
  assert.equal(mustScanLocally(['deny.toml']), true);
  assert.equal(mustScanLocally(['.github/workflows/akula-dependency-smoke.yml']), true);
  assert.equal(mustScanLocally(['README.md']), false);
});

test('one-time security bootstrap allowlist cannot absorb application changes', () => {
  const { BOOTSTRAP_FILES } = require('./dependency-security-gate.cjs');
  assert.deepEqual([...BOOTSTRAP_FILES].sort(), [
    '.github/scripts/dependency-security-gate.cjs',
    '.github/scripts/dependency-security-gate.test.cjs',
    '.github/workflows/ci.yml',
    '.github/workflows/dependency-security.yml',
  ].sort());
  assert.equal(BOOTSTRAP_FILES.has('Cargo.lock'), false);
  assert.equal(BOOTSTRAP_FILES.has('iroh/src/endpoint.rs'), false);
});

test('repo-wide main listing admits only the exact watchdog workflow path', () => {
  const target = '.github/workflows/dependency-security.yml';
  const runs = [
    { id: 9, name: 'Akula dependency security watchdog', path: '.github/workflows/lookalike.yml', event: 'push', head_branch: 'main' },
    { id: 8, path: target, event: 'pull_request', head_branch: 'main' },
    { id: 7, path: target, event: 'push', head_branch: 'feature' },
    { id: 6, path: target, event: 'push', head_branch: 'main' },
    { id: 5, path: target, event: 'schedule', head_branch: 'main' },
  ];
  assert.deepEqual(watchdogRuns(runs).map(x => x.id), [6, 5]);
  assert.deepEqual(watchdogRuns([{ id: 4, event: 'push', head_branch: 'main' }]), []);
  assert.deepEqual(watchdogRuns([]), []);
  assert.throws(() => watchdogRuns(null), /missing workflow_runs/);
});

test('newest failed watchdog stays a HOLD in exact-path-filtered runs', () => {
  const target = '.github/workflows/dependency-security.yml';
  const runs = watchdogRuns([
    { ...run('failure'), id: 12, path: target, event: 'push' },
    { ...run(), id: 11, path: target, event: 'push' },
  ]);
  assert.deepEqual(runs.map(r => r.id), [12, 11]);
  assert.equal(verdict(runs[0], [job('failure')], now).kind, 'deny');
});

test('paged scan finds an actual audited run after more than 100 unrelated runs', async () => {
  const noise = Array.from({ length: 100 }, (_, i) => ({
    id: 1000 - i, path: '.github/workflows/ci.yml',
    event: 'push', head_branch: 'main',
  }));
  const seen = [];
  const read = async url => {
    seen.push(url);
    if (new URL(url).searchParams.get('page') === '1') return { workflow_runs: noise };
    if (new URL(url).searchParams.get('page') === '2') return { workflow_runs: [{
      ...run(), id: 25, path: '.github/workflows/dependency-security.yml',
    }] };
    if (url.includes('/25/jobs?')) return { jobs: [job()] };
    throw new Error('unexpected GitHub API: ' + url);
  };
  assert.match(await checkLatestAudit('calex794/roha', 'test', read, now), /run 25/);
  assert.equal(seen.length, 3);
});

test('a skipped decision is never substituted for actual RustSec success', async () => {
  const read = async url => {
    if (url.includes('/actions/runs?')) return { workflow_runs: [
      { ...run(), id: 20, path: '.github/workflows/dependency-security.yml' },
      { ...run(), id: 19, path: '.github/workflows/dependency-security.yml' },
    ] };
    if (url.includes('/20/jobs?')) return { jobs: [job('skipped')] };
    if (url.includes('/19/jobs?')) return { jobs: [job()] };
    throw new Error('unexpected GitHub API: ' + url);
  };
  assert.match(await checkLatestAudit('calex794/roha', 'test', read, now), /run 19/);
});

test('a newer failed audit blocks an older green result', async () => {
  const read = async url => {
    if (new URL(url).searchParams.get('page') === '1') return { workflow_runs: [
      { ...run('failure'), id: 40, path: '.github/workflows/dependency-security.yml' },
      ...Array.from({ length: 99 }, (_, i) => ({
        id: 900 - i, event: 'push', head_branch: 'main', path: '.github/workflows/ci.yml',
      })),
    ] };
    if (url.includes('/40/jobs?')) return { jobs: [job('failure')] };
    throw new Error('must not reach older green evidence');
  };
  await assert.rejects(checkLatestAudit('calex794/roha', 'test', read, now), /failed or unavailable/);
});

test('GitHub API errors and absent audit evidence must HOLD', async () => {
  const denied = async () => { throw new Error('GitHub API HTTP 403'); };
  await assert.rejects(checkLatestAudit('calex794/roha', 'test', denied, now), /HTTP 403/);
  const empty = async () => ({ workflow_runs: [] });
  await assert.rejects(checkLatestAudit('calex794/roha', 'test', empty, now), /no fresh actual RustSec audit/);
});

/**
 * End-to-end regression: actually execute the SAME CLI used by the required
 * protected "cargo deny" job. Controlled GitHub Actions API responses are
 * injected only in a child process. --base=HEAD --head=HEAD means an ordinary
 * non-dependency PR: no local security-policy remediation exemption.
 *
 * Never mutate the real main watchdog state, branch rules or Cargo.lock.
 */
const { spawnSync, execFileSync } = require('node:child_process');
const path = require('node:path');
const childMockPath = path.resolve(__dirname, 'dependency-security-gate.mock.cjs');
const childGatePath = path.resolve(__dirname, 'dependency-security-gate.cjs');

function invokeSecurityGateWithMock(scenario) {
  const head = execFileSync('git', ['rev-parse', 'HEAD'], {
    encoding: 'utf8',
    cwd: process.cwd(),
  }).trim();
  const result = spawnSync(process.execPath, [
    childGatePath, '--base=' + head, '--head=' + head,
  ], {
    cwd: process.cwd(),
    encoding: 'utf8',
    timeout: 15_000,
    env: {
      ...process.env,
      GITHUB_REPOSITORY: 'calex794/roha',
      GH_TOKEN: 'fixture-only-token',
      ROHA_RUSTSEC_E2E_SCENARIO: scenario,
      NODE_OPTIONS: '--require=' + childMockPath,
    },
  });
  assert.equal(result.error, undefined, result.stderr);
  return result;
}

test('required cargo-deny CLI E2E: current verified audit passes without local scan', () => {
  const result = invokeSecurityGateWithMock('fresh');
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stderr, /SECURITY: fresh successful RustSec scan in run 204/);
  assert.match(result.stdout, /scan_required=false/);
  assert.match(result.stdout, /bootstrap_mode=false/);
});

test('required cargo-deny CLI E2E: newest skipped day uses only an older real green audit', () => {
  const result = invokeSecurityGateWithMock('skipped_then_green');
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stderr, /fresh successful RustSec scan in run 203/);
  assert.match(result.stdout, /scan_required=false/);
});

test('required cargo-deny CLI E2E: page 2 can contain the latest real watchdog', () => {
  const result = invokeSecurityGateWithMock('page_two');
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stderr, /fresh successful RustSec scan in run 204/);
});

for (const [scenario, expected] of [
  ['stale', /older than 96 hours/],
  ['future_timestamp', /invalid watchdog audit completion timestamp/],
  ['failed_newest', /watchdog audit failed or unavailable/],
  ['in_progress', /not completed/],
  ['missing_job', /watchdog audit job missing/],
  ['only_skipped', /no fresh actual RustSec audit/],
  ['wrong_workflow', /no fresh actual RustSec audit/],
  ['empty_runs', /no fresh actual RustSec audit/],
  ['http_403', /GitHub Actions API HTTP 403/],
  ['job_http_403', /GitHub Actions API HTTP 403/],
  ['network_unavailable', /Simulated GitHub API outage/],
  ['malformed_api', /missing workflow_runs/],
]) {
  test('required cargo-deny CLI E2E: ' + scenario + ' must fail closed', () => {
    const result = invokeSecurityGateWithMock(scenario);
    assert.equal(result.status, 1,
      'Expected mandatory HOLD. stdout=' + result.stdout + ' stderr=' + result.stderr);
    assert.match(result.stderr, expected);
    assert.doesNotMatch(result.stdout, /scan_required=true|bootstrap_mode=true/,
      'Unrelated PR must not gain a remediation exemption');
  });
}
