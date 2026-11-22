#!/usr/bin/env bash
# Runs CMD in a delegated cgroup v2, and gives an empty child cgroup for the jails of the worker
# in STRATA_CGROUP (tech design 0049, change 3). nsjail needs it for the limits of 0007 with no root.
# With CGROUP_ROOT unset, the script starts a delegated scope of the systemd user manager.
# CI sets CGROUP_ROOT to a cgroup that it gave to the user, with the shell of the step in it.
# Usage: scripts/in-cgroup.sh CMD...
set -euo pipefail
if [ -z "${CGROUP_ROOT:-}" ]; then
  CGROUP_ROOT=delegated-scope exec systemd-run --user --scope --quiet -p Delegate=yes -- "$0" "$@"
fi
if [ "$CGROUP_ROOT" = delegated-scope ]; then
  CGROUP_ROOT=/sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)
fi
# Linux enables controllers only in a cgroup with no process, so this shell moves to a leaf first.
mkdir -p "$CGROUP_ROOT/main" "$CGROUP_ROOT/jails"
echo $$ >"$CGROUP_ROOT/main/cgroup.procs"
echo "+memory +pids +cpu" >"$CGROUP_ROOT/cgroup.subtree_control"
export STRATA_CGROUP=$CGROUP_ROOT/jails
exec "$@"
