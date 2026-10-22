#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?A scoped GitHub Actions token is required}"
: "${REPO:?Repository is required}"
: "${TARGET_BRANCH:?Explicit branch name is required}"
: "${EXPECTED_SHA:?Exact branch-tip SHA is required}"
: "${MODE:?Expected DRY_RUN or ARCHIVE}"
: "${DEFAULT_BRANCH:?Default branch is required}"

if [[ ! "${EXPECTED_SHA}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "::error::Expected 40-digit lowercase hexadecimal SHA"
  exit 1
fi
if [[ ! "${TARGET_BRANCH}" =~ ^[A-Za-z0-9._/-]+$ ]]; then
  echo "::error::Unsafe branch-ref syntax"
  exit 1
fi
if [[ "${TARGET_BRANCH}" == "${DEFAULT_BRANCH}" ]]; then
  echo "::error::The default branch cannot be archived"
  exit 1
fi
case "${TARGET_BRANCH}" in
  ci/*|feature/*|fix/*|lab/*|repro/*|experiment/*|integration/*|sync/*|ops/*|test/*|chore/*) ;;
  *) echo "::error::Not an eligible short-lived branch"; exit 1 ;;
esac
case "${MODE}" in
  DRY_RUN) ;;
  ARCHIVE)
    if [[ "${CONFIRMATION:-}" != "ARCHIVE" ]]; then
      echo "::error::Explicit ARCHIVE confirmation is required"
      exit 1
    fi
    ;;
  *) echo "::error::Unknown mode"; exit 1 ;;
esac

check_open_pr_dependencies() {
  local refs base head head_repo
  refs="$(gh api --paginate "repos/${REPO}/pulls?state=open&per_page=100" \
    --jq '.[] | [.base.ref, .head.ref, (.head.repo.full_name // "")] | @tsv')"
  while IFS=$'\t' read -r base head head_repo; do
    if [[ "${base}" == "${TARGET_BRANCH}" ]] || \
       { [[ "${head}" == "${TARGET_BRANCH}" ]] && [[ "${head_repo}" == "${REPO}" ]]; }; then
      echo "::error::An open PR uses this branch as head or base"
      return 1
    fi
  done <<< "${refs}"
}

current="$(gh api "repos/${REPO}/git/ref/heads/${TARGET_BRANCH}" --jq '.object.sha')"
protected="$(gh api "repos/${REPO}/branches/${TARGET_BRANCH}" --jq '.protected')"
if [[ "${current}" != "${EXPECTED_SHA}" ]] || [[ "${protected}" != false ]]; then
  echo "::error::Branch moved, is protected, or cannot be verified"
  exit 1
fi
check_open_pr_dependencies

archive_tag="archive/${TARGET_BRANCH}/${EXPECTED_SHA:0:12}"

# A missing single-ref API result is HTTP 404; gh --jq can still print 'null'.
# Use the matching-refs collection (HTTP 200, [] on absent), then filter
# the exact full tag name. API errors and ambiguous values must fail closed.
lookup_archive_sha() {
  local response
  response="$(gh api "repos/${REPO}/git/matching-refs/tags/${archive_tag}")" || {
    echo "::error::Unable to list archive tags; branch remains intact" >&2
    return 1
  }
  jq -r --arg full_ref "refs/tags/${archive_tag}" '
    if type != "array" then error("unexpected archive ref listing")
    else
      [.[] | select(.ref == $full_ref) | .object.sha] |
      if length == 0 then ""
      elif length == 1 and
           (.[0] | type) == "string" and
           (.[0] | test("^[0-9a-f]{40}$"))
      then .[0]
      else error("ambiguous or invalid exact archive ref")
      end
    end
  ' <<< "${response}"
}
existing="$(lookup_archive_sha)" || exit 1
if [[ -n "${existing}" && "${existing}" != "${EXPECTED_SHA}" ]]; then
  echo "::error::Archive tag already exists with a different commit"
  exit 1
fi

if [[ "${MODE}" == DRY_RUN ]]; then
  printf 'DRY RUN: eligible branch %s at %s, proposed tag %s. No changes made.\n' \
    "${TARGET_BRANCH}" "${EXPECTED_SHA}" "${archive_tag}" >> "${GITHUB_STEP_SUMMARY}"
  exit 0
fi

# Ref creation is atomic: preserve the commit before attempting any deletion.
if [[ -z "${existing}" ]]; then
  gh api --method POST "repos/${REPO}/git/refs" \
    -f ref="refs/tags/${archive_tag}" -f sha="${EXPECTED_SHA}" --silent
fi
tag_sha="$(lookup_archive_sha)" || exit 1
if [[ "${tag_sha}" != "${EXPECTED_SHA}" ]]; then
  echo "::error::Archive tag did not verify; branch remains intact"
  exit 1
fi

# Recheck dependencies before a destructive branch-ref operation.
check_open_pr_dependencies
updated="$(gh api "repos/${REPO}/git/ref/heads/${TARGET_BRANCH}" --jq '.object.sha')"
if [[ "${updated}" != "${EXPECTED_SHA}" ]]; then
  echo "::error::Branch moved after archival; original remains intact"
  exit 1
fi

gh auth setup-git
cd "$(mktemp -d)"
git init -q
git remote add origin "https://github.com/${REPO}.git"
git push --porcelain \
  --force-with-lease="refs/heads/${TARGET_BRANCH}:${EXPECTED_SHA}" \
  origin ":refs/heads/${TARGET_BRANCH}"
printf 'Archived %s at %s as refs/tags/%s; only the branch ref was removed.\n' \
  "${TARGET_BRANCH}" "${EXPECTED_SHA}" "${archive_tag}" >> "${GITHUB_STEP_SUMMARY}"
