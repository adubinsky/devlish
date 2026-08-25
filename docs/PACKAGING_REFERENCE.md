# Devlish Packaging Reference
Last updated: 2026-03-23

## Purpose

`devlish package` builds a runnable artifact directory for a `.dvl` program.

The package format is intentionally simple:
- original Devlish source
- compiled Ruby or JavaScript entrypoint
- `manifest.json`
- `run` launcher
- bundled `assets/` for workflow document inputs

## Commands

Workflow package:

```bash
devlish package examples/tutorial/03_branch_and_route.dvl --target ruby
```

Class-style package:

```bash
devlish package examples/class_style/01_payroll_calculator.dvl --target javascript --method calculate_wages --args '[40,25]'
```

Custom output directory:

```bash
devlish package examples/tutorial/03_branch_and_route.dvl --target javascript --output-dir tmp/review-package
```

## Artifact Layout

Typical output:

```text
pkg/review_flow-ruby/
  manifest.json
  program.dvl
  program.rb
  run
  assets/
    review_packet.txt
```

## Notes

- Workflow packages copy loaded document assets into the package and rewrite
  load paths to package-local assets.
- Relative document paths in compiled output are resolved relative to the
  packaged script location, not the caller's current directory.
- Class-style packages require `--method` so the generated launcher has a
  default invocation target.
- Packaged Ruby artifacts require `ruby`.
- Packaged JavaScript artifacts require `node`.
