# Stop hook: handover gate

When the session is about to end, check:

1. Is there an in-flight spec (a specs/<feature-dir>/ with Status Draft or Accepted
   and unchecked implementation tasks)?
2. If yes: does the latest handover file in that directory have today's date?

If there is in-flight work and no current handover, BLOCK the stop and instruct
the agent to run /speckit.handover.write first.

A session that ends mid-cycle without a handover loses context that cannot be
recovered. The handover is the baton.
