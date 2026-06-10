# Fixture Project

A tiny deterministic source tree used by the ctx-mcp differential parity oracle.
It contains a single Go file (`main.go`) with a type and two functions, plus a
plain-text note. This file doubles as the `ctx://docs/readme` resource backing
file so `resources/list` and `resources/read` have a deterministic target.
