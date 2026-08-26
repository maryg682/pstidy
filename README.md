# pstidy

Process listings that show parent/child relationships end up in all
sorts of shapes depending on where they came from: `ps -eo pid,ppid,comm`
output pasted into a bug report, a log line with tabs instead of spaces,
a support ticket where someone reformatted it by hand. The pid/ppid
relationships are all there, but the layout is inconsistent and hard to
scan.

pstidy reads that kind of line-oriented pid/ppid data and prints it back
out as one consistently formatted tree, regardless of how messy the
input whitespace or ordering was.

## Input format

One process per line: `pid ppid command`. Extra whitespace is ignored,
blank lines and `#` comments are skipped, and the command can contain
spaces (everything after the second field is taken as the command).

```
# messy on purpose - stray indentation, blank lines, extra spaces
  810   1     sshd

1  0  init
2200 810 bash
2350   2200    vim
900 1 cron
```

This is exactly the shape you get from:

```
ps -eo pid,ppid,comm --no-headers
```

## Usage

From a file:

```
pstidy processes.txt
```

From stdin, piped straight from `ps`:

```
ps -eo pid,ppid,comm --no-headers | pstidy
```

Multiple files are concatenated in the order given.

## Output

The example input above becomes:

```
init (1)
├── sshd (810)
│   └── bash (2200)
│       └── vim (2350)
└── cron (900)
```

A record is treated as a root if its ppid is 0 or doesn't match any
pid present in the input - that covers both real init processes and
partial dumps where a parent got cut off. Output is always sorted by
pid, so the same input produces the same tree every time.

## Building

Standard library only, no dependencies:

```
cargo build --release
```
