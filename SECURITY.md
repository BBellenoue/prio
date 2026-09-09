# Security policy

## Supported versions

Only the current `main` receives fixes. There is no maintenance branch.

## Reporting a vulnerability

Report privately, not through a public issue: open the repository's
[**Security** tab](https://github.com/BBellenoue/prio/security/advisories/new)
and use *Report a vulnerability*. The report stays visible only to you and the
maintainers until a fix is published.

Useful in a report:

- what an attacker gains, and what they need to already have
- the affected file or code path
- the steps that demonstrate it

Expect an acknowledgement within a week. This is a small project maintained on
free time: there is no bounty, and fixes ship when they are ready. Please hold
public disclosure until a fix is out, or for 90 days, whichever comes first.

## Scope

Prio is a local application. It opens no network connection, stores its data
as plain JSON under `%APPDATA%\prio`, and runs with the rights of the user who
launched it. Anything that lets another local user or process read or alter
that data through Prio, or that lets a crafted `tasks.json` do more than
display badly, is in scope.
