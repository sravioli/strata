#!/usr/bin/env bash
# CI only: makes sure that NSJAIL can make a user namespace on the runner (tech design 0049, change 3).
# When AppArmor blocks it (kernel.apparmor_restrict_unprivileged_userns is 1), the script loads a
# profile that permits user namespaces for NSJAIL only. It never changes the sysctl.
# Usage: scripts/ci-nsjail-userns.sh NSJAIL
set -euo pipefail
nsjail=$(realpath "${1:?usage: ci-nsjail-userns.sh NSJAIL}")
probe() { "$nsjail" -Mo -Q --chroot / -- /bin/true; }

echo "kernel.apparmor_restrict_unprivileged_userns: $(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null || echo absent)"
if probe; then
  echo "nsjail makes a user namespace with no profile"
  exit 0
fi
sudo tee /etc/apparmor.d/strata-nsjail >/dev/null <<PROFILE
abi <abi/4.0>,
include <tunables/global>
profile strata-nsjail $nsjail flags=(unconfined) {
  userns,
}
PROFILE
sudo apparmor_parser -r /etc/apparmor.d/strata-nsjail
probe
echo "nsjail makes a user namespace with the AppArmor profile strata-nsjail"
