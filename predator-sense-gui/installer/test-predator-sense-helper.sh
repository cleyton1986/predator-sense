#!/bin/bash
set -euo pipefail

if [ "$EUID" -eq 0 ]; then
  echo 'run this test as an unprivileged user; refusing to touch the real sysfs' >&2
  exit 77
fi

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
HELPER="$SCRIPT_DIR/predator-sense-helper"
FIXTURE=$(mktemp -d)
trap 'chmod -R u+w "$FIXTURE" 2>/dev/null || true; rm -rf "$FIXTURE"' EXIT

write_fixture() {
  local relative=$1 value=$2 path="$FIXTURE/$1"
  mkdir -p "${path%/*}"
  printf '%s\n' "$value" > "$path"
}

read_fixture() {
  local value
  IFS= read -r value < "$FIXTURE/$1"
  printf '%s' "$value"
}

assert_value() {
  local relative=$1 expected=$2 actual
  actual=$(read_fixture "$relative")
  if [ "$actual" != "$expected" ]; then
    printf 'expected %s=%s, got %s\n' "$relative" "$expected" "$actual" >&2
    exit 1
  fi
}

run_helper() {
  PREDATOR_SENSE_HELPER_TEST_ROOT="$FIXTURE" "$HELPER" "$@"
}

make_policy() {
  local index=$1 driver=${2:-intel_pstate} with_epp=${3:-yes}
  local policy="devices/system/cpu/cpufreq/policy$index"
  write_fixture "$policy/scaling_driver" "$driver"
  write_fixture "$policy/scaling_governor" powersave
  write_fixture "$policy/scaling_available_governors" 'performance powersave'
  if [ "$with_epp" = yes ]; then
    write_fixture "$policy/energy_performance_preference" balance_performance
    write_fixture "$policy/energy_performance_available_preferences" \
      'default performance balance_performance balance_power power'
  fi
}

make_policy 0
make_policy 1
write_fixture devices/system/cpu/intel_pstate/status active
write_fixture devices/system/cpu/intel_pstate/no_turbo 0
write_fixture devices/system/cpu/intel_pstate/min_perf_pct 17

# Active intel_pstate HWP performance path: EPP must be written before the
# governor and all values must be applied across every policy.
run_helper apply-cpu-profile performance 0 0 50
for index in 0 1; do
  assert_value "devices/system/cpu/cpufreq/policy$index/scaling_governor" performance
  assert_value "devices/system/cpu/cpufreq/policy$index/energy_performance_preference" 0
done
assert_value devices/system/cpu/intel_pstate/no_turbo 0
assert_value devices/system/cpu/intel_pstate/min_perf_pct 50

# Leaving performance reverses the order so a named, non-zero EPP is writable.
run_helper apply-cpu-profile powersave balance_performance 0 17
for index in 0 1; do
  assert_value "devices/system/cpu/cpufreq/policy$index/scaling_governor" powersave
  assert_value "devices/system/cpu/cpufreq/policy$index/energy_performance_preference" balance_performance
done

# A failure after policy0 changed must restore its snapshot and return an
# actionable path instead of the old empty "Helper failed:" diagnostic.
chmod u-w "$FIXTURE/devices/system/cpu/cpufreq/policy1/scaling_governor"
if output=$(run_helper apply-cpu-profile performance 0 0 50 2>&1); then
  echo 'expected the read-only policy write to fail' >&2
  exit 1
fi
case "$output" in
  *policy1/scaling_governor*"rolling back CPU profile"*) ;;
  *) printf 'missing actionable rollback diagnostic: %s\n' "$output" >&2; exit 1 ;;
esac
assert_value devices/system/cpu/cpufreq/policy0/scaling_governor powersave
assert_value devices/system/cpu/cpufreq/policy0/energy_performance_preference balance_performance
chmod u+w "$FIXTURE/devices/system/cpu/cpufreq/policy1/scaling_governor"

# Generic cpufreq systems without EPP/intel_pstate controls use explicit skip
# values instead of failing a glob against missing Intel-only attributes.
rm -rf "$FIXTURE"
mkdir -p "$FIXTURE"
make_policy 0 acpi-cpufreq no
run_helper apply-cpu-profile performance skip skip skip
assert_value devices/system/cpu/cpufreq/policy0/scaling_governor performance

echo 'predator-sense-helper integration tests passed'
