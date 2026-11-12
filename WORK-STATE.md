# Whispering Intel work state

Roger approved implementation, builds/tests, and incremental commits/pushes on 12 November 2026. Do not request routine approval again. Read root and app AGENTS.md and relevant rules.

## Agreed scope

- Persistent Dock app: close hides, Dock reopens, explicit Quit exits.
- Non-focus-stealing floating indicator: real microphone waveform, distinct transcribing/cleanup animation, clear errors.
- Separate configurable Enter action; no automatic submission by default.
- Optional assignable mouse buttons, blank by default. Parallel keyboard/mouse actions. Suppress assigned mouse button's normal behaviour. Native macOS support and permission failure handling required.
- Explicit LLM cleanup toggle, off by default; automatic transformation bypassed when off.
- Intel macOS builds only. Short honest README; preserve upstream license/attribution.
- Deferred: chunked/10-second transcription, incremental pasting, speed optimisation, security-hardening project.

## Current state

- Fork: https://github.com/archsinit/epicenter
- Branch: whispering-intel, based on upstream v7.10.0.
- First implementation: explicit off-by-default cleanup toggle in Transcription settings, independent configurable Enter command (keyboard defaults blank), native macOS close-to-hide and Dock reopen handlers. Four cleanup-policy tests pass. Dock code not compiled/tested on macOS yet.
- Bun 1.3.3 installed at /home/piweb/.local/bin/bun (add directory to PATH). Dependencies installed. Upstream lockfile had stale app version; install required refreshing it (small transitive changes also recorded). No Cargo on PATH; native macOS validation cannot run here.
- Frontend production build PASSED on Linux. App svelte-check reports 31 errors/17 warnings; an untouched v7.10.0 worktree with the same dependencies reports 33 errors/20 warnings. Do not claim type checking passes; baseline problems need triage. Logs: /tmp/whispering-check.log, /tmp/whispering-baseline-check.log, /tmp/whispering-build.log. Baseline worktree: /tmp/whispering-intel-baseline.
- Scheduler: workspace pi-web/whispering-work.cjs; cron every 30 minutes. Target is this chat. Idle-only double state check, no intentional queuing/steering, fail closed on unknown state, slot receipt avoids duplicates. No session rollover. Server API lacks atomic idle-only admission, so a narrow race remains. Five offline tests passed; live read-only check correctly reported busy. Touch workspace pi-web/whispering-work.paused to pause. Never commit session targets, receipts, or logs.

## Next steps

1. Review/extend cleanup and Enter coverage; ensure automatic Enter remains off by default.
2. Validate native macOS close/hide and Dock reopen in Intel CI and on the user's Mac.
3. Implement floating indicator and real audio levels from existing recorder (no second microphone stream).
4. Implement native suppressible optional mouse bindings with independent keyboard shortcuts.
5. Intel-only fork release workflow, own app identity/update policy, short README.
6. Validate frontend checks/builds, native compilation in CI, tests and review. Commit/push incremental changes. Clearly distinguish what requires Roger's Intel Mac testing.

Pause the scheduler once all server-implementable work is complete and only user Mac testing remains, or if the user asks to stop. Do not repeatedly consume model calls when no useful work is possible.
