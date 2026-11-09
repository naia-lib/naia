#!/usr/bin/env bash
# ONE_SOURCE_OF_TRUTH placement gate (Connor via Ada 188238, ambry standards/ONE_SOURCE_OF_TRUTH.md).
# A repo carries its own behavior specs plus an AGENTS.md line naming them;
# missions/handoffs/plans live in a dated ARCHIVE with a README, never at top level.
# RED until the five _AGENTS mission files are archived and ARCHIVE has a README.
set -u
fail=0
for f in MISSION_026_SOCKET_SIMPLIFICATION.md MISSION_HANDOFF_NAIA_SESSION.md \
         MISSION_PIPELINE_API_BOUNDARY.md MISSION_PIPELINE_ELEGANCE.md \
         MISSION_SIM_PIPELINED_NAMAKO_BDD.md; do
  if [ -e "_AGENTS/$f" ]; then echo "RED: _AGENTS/$f not archived"; fail=1; fi
  if [ ! -e "_AGENTS/ARCHIVE/$f" ]; then echo "RED: _AGENTS/ARCHIVE/$f missing"; fail=1; fi
done
if [ ! -e "_AGENTS/ARCHIVE/README.md" ]; then echo "RED: _AGENTS/ARCHIVE/README.md missing"; fail=1; fi
if [ "$fail" -eq 0 ]; then echo "GREEN: mission files archived with README"; fi
exit "$fail"
