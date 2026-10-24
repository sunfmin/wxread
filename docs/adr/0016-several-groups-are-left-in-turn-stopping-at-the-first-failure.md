# Several groups are left in turn, stopping at the first failure, and a group already left is passed over

The account's owner wants to leave every group that is only ads, which can be dozens. `wxread group leave` takes several groups (on its line, or on stdin with `-`) and leaves them one after another, each exactly as 0015 leaves one: found and checked on WeChat's window before, confirmed in the databases after.

## How

- **Every group is read before any is touched.** A group that cannot be left (another of the account's groups is shown as it is, an id that is no group) refuses the whole run, every one named at once, and nothing is touched; so do the blockers --yes needs. A typo in the twentieth id does not leave nineteen groups first.
- **The first failure stops the run.** A screen that does not read as expected, or a leave the databases do not confirm, means something about WeChat or the Mac is not as the path expects, and the next group would meet the same. The answer says of each group whether it was left, stopped the run (and why, in its error), or was not tried, and exits 1.
- **A group the account is not in is passed over, not refused.** Its outcome is `not_member`: there is nothing to leave. So the same command, run again once the cause is fixed, leaves only the groups still to leave: what is left is read from the databases again, not remembered, as every run reads the world again.
- **wxread suggests nothing and leaves nothing by itself.** `wxread chats` answers of each group who sent the most of its newest messages, how many, and whether the account ever spoke in it: enough for an agent to suggest groups that are only ads. Only the owner picks which to leave, and only --yes leaves them.

## Considered

- **Going on past a failure, to leave as many as possible.** A failure on one group's screen is most often the window being in a state the path does not know (a dialog, the person at the Mac), which the next group meets too; going on would click through a window wxread no longer understands. Stopping keeps "anything not read as expected stops the path" (0015) true of the whole run.
- **Refusing a group the account is not in, as one leave did.** Then the same command run after a stop refuses the groups it already left, and the agent must work out the rest itself. Passing over makes the run idempotent; `wxread group <id>` still answers who is in it.

## Consequences

- An id of a group the account was never in is answered as `not_member`, not as an error: the answer, not the exit code, says it.
- A run of n groups holds WeChat's window for about ten seconds per group, waiting each time for the Mac to be left alone.
