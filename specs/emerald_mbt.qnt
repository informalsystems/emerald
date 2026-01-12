// -*- mode: Bluespec; -*-

// This spec refines the abstract Emerald spec and introduces transition
// predicates in order to guide MBT exploration into interesting/realistic
// scenarios.

module emerald_mbt {
  import basicSpells.* from "spells/basicSpells"
  import emerald as em from "emerald"

  // One failure per (node, height) pair
  pure def one_failure_filter(ctx: em::LocalContext, t: em::Transition): bool = {
    val s = t.post_state
    val ext = ctx.extensions

    match em::get_action_taken(t) {
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

  // No failures
  pure def no_failures_filter(ctx: em::LocalContext, t: em::Transition): bool =
    match em::get_action_taken(t) {
      | Failure(args) => false
      | _             => true
    }

  action init = em::init
  action step = em::step_with_filter(one_failure_filter)
  action step_no_failures = em::step_with_filter(no_failures_filter)
}
