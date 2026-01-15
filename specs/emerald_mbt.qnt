// -*- mode: Bluespec; -*-

// =============================================================================
// Emerald Model-Based Tests Specification
// =============================================================================
// 
// This is a refinement of the Emerald specification for model-based testing
// (MBT) purposes.
//
// This spec guides MBT exploration into realistic scenarios by limiting the
// amount of failures per run. Without such limitations, traces can get stuck on
// crashes or restarts, preventing the exploration from covering more diverse
// behavioral cases.
//
// Note that this spec inherits `init` and `step` from the more general Emerald
// spec. To use the filtered step transitions, run it with `--step
// step_no_failures` or `--step step_with_failures`.
//
// ```
// quint run emerald_mbt.qnt --step step_no_failures --invariant="agreement"
// ```

module emerald_mbt {
  import basicSpells.* from "spells/basicSpells"
  import emerald(NODES = List("node1", "node2", "node3")).* from "emerald"

  // A transition filter that rejects all failure actions
  pure def no_failures_filter(ctx: LocalContext, t: Transition): bool =
    match get_action_taken(t) {
      | Failure(args) => false
      | _             => true
    }

  // A transition filter that allows at most one failure per (node, height)
  // pair. Note that it rejects failures at height == 0 (uninitialized nodes).
  pure def one_failure_per_height(ctx: LocalContext, t: Transition): bool = {
    val s = t.post_state
    val ext = ctx.extensions

    match get_action_taken(t) {
      | Failure(args) => {
          val last_failure_in_height =
            ext.failures
              .find(c => and {
                c.node == s.process_id,
                c.height == s.current_height
              })

          and {
            s.current_height > 0,
            last_failure_in_height == None
          }
        }
      | _ => true
    }
  }

  action step_no_failures = step_with_filter(no_failures_filter)
  action step_with_failures = step_with_filter(one_failure_per_height)
}













