# Changelog

Dates are UTC. One line per release, what changed and why, so a reader of the diff has the
sentence beside it.

## 0.1.0-pre.1

- First version.
- A `simulator` feature, off by default, exports the simulated controller and card so a caller's
  own tests can run against them.
- The simulated controller panics after about four million register writes or commands, so a
  driver stuck in a loop fails its test at once instead of filling the host's memory.

## BUGS

- The template's lag check guards the CI pin, not this file; a change that matters to a user of
  the crate belongs in a release line here.
