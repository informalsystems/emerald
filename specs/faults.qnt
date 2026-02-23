// -*- mode: Bluespec; -*-

// =============================================================================
// Fault Events (reusable)
// =============================================================================
//
// General infrastructure fault types.
// A node crash/restart affects all APIs simultaneously — faults are
// node-level events, not embedded in individual API request types.
//
// Disk/mem convention:
//   Modules using faults should split state into { disk: D, mem: M }.
//   - Crash:   reset both disk and mem to initial values
//   - Restart: reset mem to initial, preserve disk (survives on disk)
//
//   This encodes persistence semantics in the type structure:
//     - New persistent fields → add to DiskState → auto-survive restarts
//     - New ephemeral fields → add to MemState → auto-reset on restart
//
//   Example:
//     pure def crashReset(initDisk, initMem) = { disk: initDisk, mem: initMem }
//     pure def restartReset(current, initMem) = { ...current, mem: initMem }
//
//   Request histories are mem state — cleared on fault, containing only the
//   current session's requests. This means contract properties over request
//   histories automatically scope to the current session with no
//   session-boundary logic needed.

module faults {
  type FaultEvent =
    | Crash    // Node lost all state (disk + mem)
    | Restart  // Node restarted, disk state preserved

  pure def isCrash(e: FaultEvent): bool = match e {
    | Crash => true
    | _ => false
  }
}
