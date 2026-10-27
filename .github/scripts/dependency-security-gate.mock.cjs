'use strict';

// This file is loaded ONLY by an isolated child Node process launched from
// dependency-security-gate.test.cjs. It replaces the GitHub Actions HTTP API
// and makes no outgoing request or repository mutation.
const scenario = process.env.ROHA_RUSTSEC_E2E_SCENARIO;
const prefix = '/repos/calex794/roha/actions/runs';
const workflow = '.github/workflows/dependency-security.yml';
const now = Date.now();
const stamp = ageMs => new Date(now - ageMs).toISOString();
const record = (id, extra = {}) => ({
  id, path: workflow, head_branch: 'main', event: 'push',
  status: 'completed', conclusion: 'success',
  updated_at: stamp(60_000), ...extra,
});
const audit = (conclusion = 'success', ageMs = 60_000) => ({
  name: 'RustSec dependency audit',
  conclusion,
  completed_at: stamp(ageMs),
});
const noise = Array.from({ length: 100 }, (_, id) =>
  record(1000 - id, { path: '.github/workflows/ci.yml' })
);

globalThis.fetch = async (rawUrl, request) => {
  const url = new URL(rawUrl);
  if (url.origin !== 'https://api.github.com' ||
      !url.pathname.startsWith(prefix) ||
      request?.headers?.authorization !== 'Bearer fixture-only-token') {
    throw new Error('E2E mock refused URL or credential: ' + url.pathname);
  }
  if (scenario === 'http_403') return { ok: false, status: 403 };
  if (scenario === 'network_unavailable') throw new Error('Simulated GitHub API outage');

  let body;
  if (url.pathname === prefix) {
    const page = url.searchParams.get('page');
    if (scenario === 'malformed_api') {
      body = { wrong_field: [] };
    } else if (scenario === 'page_two') {
      body = { workflow_runs: page === '1' ? noise : page === '2' ? [record(204)] : [] };
    } else if (page !== '1') {
      body = { workflow_runs: [] };
    } else {
      const cases = {
        fresh: [record(204)],
        stale: [record(204)],
        future_timestamp: [record(204)],
        failed_newest: [record(204, { conclusion: 'failure' }), record(203)],
        in_progress: [record(204, { status: 'in_progress', conclusion: null })],
        missing_job: [record(204)],
        job_http_403: [record(204)],
        only_skipped: [record(204)],
        skipped_then_green: [record(204), record(203)],
        wrong_workflow: [record(204, { path: '.github/workflows/ci.yml' })],
        empty_runs: [],
      };
      if (!Object.hasOwn(cases, scenario)) throw new Error('Unknown E2E scenario: ' + scenario);
      body = { workflow_runs: cases[scenario] };
    }
  } else {
    const match = /^\/repos\/calex794\/roha\/actions\/runs\/(\d+)\/jobs$/.exec(url.pathname);
    if (!match) throw new Error('E2E mock refused unexpected GitHub API route ' + url.pathname);
    if (scenario === 'job_http_403') return { ok: false, status: 403 };
    const id = Number(match[1]);
    if (scenario === 'missing_job') body = { jobs: [] };
    else if (scenario === 'only_skipped' || (scenario === 'skipped_then_green' && id === 204)) body = { jobs: [audit('skipped')] };
    else if (scenario === 'failed_newest' && id === 204) body = { jobs: [audit('failure')] };
    else if (scenario === 'stale') body = { jobs: [audit('success', 97 * 60 * 60 * 1000)] };
    else if (scenario === 'future_timestamp') body = { jobs: [audit('success', -10 * 60 * 1000)] };
    else body = { jobs: [audit()] };
  }
  return { ok: true, status: 200, json: async () => body };
};
