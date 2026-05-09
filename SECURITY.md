# Security policy

## Reporting a vulnerability

If you find a security issue in cerridwen — anything from a panic that
DoSes the server, to an injection vector through one of the parsers, to
a logic flaw that lets a request bypass the rate limiter — please email
**polzer@fastmail.com** with details rather than opening a public
issue. PGP welcome on request.

Include:

* A description of the issue
* A reproducer (curl command, JSON payload, etc.)
* Affected version (`cargo pkgid` or commit hash)
* Impact (data exposure, DoS, …)

I'll try to acknowledge within a week and to land a fix promptly. Once
a fix has shipped, the disclosure can be coordinated with you.

## Supported versions

Only `master` is supported. There are no LTS branches.

## Out of scope

* Anything in the bundled Swiss Ephemeris C library — report those to
  Astrodienst.
* AGPL-related licensing concerns — these aren't security issues.
* Findings against `target/` artefacts that aren't reproducible from
  source.
