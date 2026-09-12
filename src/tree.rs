use std::collections::{HashMap, HashSet};

pub struct Record {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
}

/// Column positions of pid, ppid, and the start of the command within a
/// whitespace-split line. The bare "pid ppid command" format is fixed
/// (0, 1, 2); a `ps -ef` style header shifts these around (e.g. UID
/// comes first) so the layout is detected from the header row instead
/// of assumed.
struct Layout {
    pid_idx: usize,
    ppid_idx: usize,
    cmd_idx: usize,
}

impl Layout {
    fn bare() -> Layout {
        Layout { pid_idx: 0, ppid_idx: 1, cmd_idx: 2 }
    }
}

/// Recognizes a `ps -ef`/`ps -efl` style header row by finding PID, PPID,
/// and CMD/COMMAND columns. Matching is case-insensitive and exact per
/// field, so a data line with a literal "pid" token can't be mistaken
/// for a header - PPID and CMD would also have to line up, which real
/// process data won't do.
fn detect_header(line: &str) -> Option<Layout> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let pid_idx = fields.iter().position(|f| f.eq_ignore_ascii_case("pid"))?;
    let ppid_idx = fields.iter().position(|f| f.eq_ignore_ascii_case("ppid"))?;
    let cmd_idx = fields
        .iter()
        .position(|f| f.eq_ignore_ascii_case("cmd") || f.eq_ignore_ascii_case("command"))?;
    Some(Layout { pid_idx, ppid_idx, cmd_idx })
}

/// Parses process records, tolerating extra whitespace, blank lines, and
/// '#' comments. Lines that don't have at least a numeric pid and ppid
/// at the expected columns are skipped rather than treated as errors,
/// since real ps/pstree dumps often carry stray notes.
///
/// The column layout defaults to "pid ppid command" (what
/// `ps -eo pid,ppid,comm --no-headers` produces), but if the first
/// content line looks like a `ps -ef` header, its column order is used
/// for the rest of the input instead.
pub fn parse(input: &str) -> Vec<Record> {
    let mut records = Vec::new();
    let mut layout: Option<Layout> = None;

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if layout.is_none() {
            if let Some(header_layout) = detect_header(line) {
                layout = Some(header_layout);
                // the header row itself carries no process data
                continue;
            }
            layout = Some(Layout::bare());
        }
        let layout = layout.as_ref().unwrap();

        let fields: Vec<&str> = line.split_whitespace().collect();
        let pid = match fields.get(layout.pid_idx).and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let ppid = match fields.get(layout.ppid_idx).and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };

        let command = if layout.cmd_idx < fields.len() {
            fields[layout.cmd_idx..].join(" ")
        } else {
            String::new()
        };
        let command = if command.is_empty() {
            "?".to_string()
        } else {
            command
        };

        records.push(Record { pid, ppid, command });
    }

    records
}

/// Connector glyphs used to draw the tree. Unicode is the default; ascii
/// exists for terminals/fonts/log viewers that mangle box-drawing chars.
struct Connectors {
    branch: &'static str,
    last_branch: &'static str,
    vertical: &'static str,
    blank: &'static str,
}

const UNICODE_CONNECTORS: Connectors = Connectors {
    branch: "├── ",
    last_branch: "└── ",
    vertical: "│   ",
    blank: "    ",
};

const ASCII_CONNECTORS: Connectors = Connectors {
    branch: "|-- ",
    last_branch: "`-- ",
    vertical: "|   ",
    blank: "    ",
};

/// Renders records as a tree, ├──/└── style by default (or ascii, see
/// `format_ascii`), sorted by pid at every level so the same input
/// always produces the same output.
pub fn format(records: &[Record]) -> String {
    format_with(records, &UNICODE_CONNECTORS)
}

/// Same as `format`, but draws connectors with plain ascii characters
/// instead of unicode box-drawing glyphs.
pub fn format_ascii(records: &[Record]) -> String {
    format_with(records, &ASCII_CONNECTORS)
}

/// Deduplicates by pid (first occurrence wins) and splits records into a
/// parent-to-children map plus the list of roots - a record is a root if
/// its parent is missing from the input (pid 0, a ppid we were never
/// given a line for, or its own pid).
struct Built<'a> {
    unique: Vec<&'a Record>,
    pids: HashSet<u32>,
    children: HashMap<u32, Vec<u32>>,
    roots: Vec<u32>,
}

fn build(records: &[Record]) -> Built {
    let mut seen = HashSet::new();
    let mut unique: Vec<&Record> = Vec::new();
    for r in records {
        if seen.insert(r.pid) {
            unique.push(r);
        }
    }

    let pids: HashSet<u32> = unique.iter().map(|r| r.pid).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut roots: Vec<u32> = Vec::new();

    for r in &unique {
        if r.ppid == r.pid || !pids.contains(&r.ppid) {
            roots.push(r.pid);
        } else {
            children.entry(r.ppid).or_default().push(r.pid);
        }
    }

    roots.sort_unstable();
    for kids in children.values_mut() {
        kids.sort_unstable();
    }

    Built { unique, pids, children, roots }
}

fn format_with(records: &[Record], connectors: &Connectors) -> String {
    let built = build(records);
    let by_pid: HashMap<u32, &Record> = built.unique.iter().map(|r| (r.pid, *r)).collect();

    let mut out = String::new();
    for (i, root) in built.roots.iter().enumerate() {
        let is_last = i + 1 == built.roots.len();
        write_node(*root, "", is_last, true, &by_pid, &built.children, connectors, &mut out);
    }
    out
}

/// Problems found in the input that a plain tree render would otherwise
/// hide: pids that name themselves as their own parent, pids whose ppid
/// doesn't match anything else in the input, and ppid cycles (pid A's
/// ancestry loops back through pid A without ever reaching a root). All
/// three are treated as roots or dropped silently by `format`, so this
/// exists to let a caller surface them instead.
pub struct Diagnostics {
    pub self_parented: Vec<u32>,
    pub missing_parent: Vec<(u32, u32)>,
    pub cycles: Vec<u32>,
}

impl Diagnostics {
    pub fn is_empty(&self) -> bool {
        self.self_parented.is_empty() && self.missing_parent.is_empty() && self.cycles.is_empty()
    }
}

pub fn diagnose(records: &[Record]) -> Diagnostics {
    let built = build(records);

    let mut self_parented = Vec::new();
    let mut missing_parent = Vec::new();
    for r in &built.unique {
        if r.ppid == r.pid {
            self_parented.push(r.pid);
        } else if !built.pids.contains(&r.ppid) {
            missing_parent.push((r.pid, r.ppid));
        }
    }
    self_parented.sort_unstable();
    missing_parent.sort_unstable();

    // Any pid not reachable from a root by walking down `children` is
    // stuck in a ppid cycle - every node in a cycle has exactly one
    // outgoing ppid edge, and none of them lead back to a root, so
    // `format` never visits them either.
    let mut reachable: HashSet<u32> = HashSet::new();
    let mut stack: Vec<u32> = built.roots.clone();
    while let Some(pid) = stack.pop() {
        if reachable.insert(pid) {
            if let Some(kids) = built.children.get(&pid) {
                stack.extend(kids.iter().copied());
            }
        }
    }

    let mut cycles: Vec<u32> = built
        .pids
        .iter()
        .copied()
        .filter(|p| !reachable.contains(p))
        .collect();
    cycles.sort_unstable();

    Diagnostics { self_parented, missing_parent, cycles }
}

fn write_node(
    pid: u32,
    prefix: &str,
    is_last: bool,
    is_root: bool,
    by_pid: &HashMap<u32, &Record>,
    children: &HashMap<u32, Vec<u32>>,
    connectors: &Connectors,
    out: &mut String,
) {
    let record = match by_pid.get(&pid) {
        Some(r) => r,
        None => return,
    };

    if !is_root {
        out.push_str(prefix);
        out.push_str(if is_last { connectors.last_branch } else { connectors.branch });
    }
    out.push_str(&record.command);
    out.push_str(" (");
    out.push_str(&record.pid.to_string());
    out.push_str(")\n");

    let child_prefix = if is_root {
        String::new()
    } else {
        format!(
            "{}{}",
            prefix,
            if is_last { connectors.blank } else { connectors.vertical }
        )
    };

    if let Some(kids) = children.get(&pid) {
        for (i, kid) in kids.iter().enumerate() {
            let last = i + 1 == kids.len();
            write_node(*kid, &child_prefix, last, false, by_pid, children, connectors, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_line() {
        let records = parse("1 0 init");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].pid, 1);
        assert_eq!(records[0].ppid, 0);
        assert_eq!(records[0].command, "init");
    }

    #[test]
    fn parse_tolerates_whitespace_blanks_and_comments() {
        let input = "\n# a comment\n  810   1     sshd  \n\n1  0  init\n";
        let records = parse(input);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].pid, 810);
        assert_eq!(records[0].ppid, 1);
        assert_eq!(records[0].command, "sshd");
        assert_eq!(records[1].pid, 1);
    }

    #[test]
    fn parse_joins_multi_word_command() {
        let records = parse("42 1 /usr/bin/env python3 -m http.server");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].command, "/usr/bin/env python3 -m http.server");
    }

    #[test]
    fn parse_defaults_missing_command_to_question_mark() {
        let records = parse("1 0");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].command, "?");
    }

    #[test]
    fn parse_skips_lines_missing_numeric_fields() {
        let input = "not a pid line\n1\n1 also-not-numeric foo\n2 0 ok\n";
        let records = parse(input);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].pid, 2);
    }

    #[test]
    fn parse_detects_ps_ef_header_and_reorders_columns() {
        let input = "UID        PID  PPID  C STIME TTY          TIME CMD\n\
                      root         1     0  0 08:00 ?        00:00:01 /sbin/init\n\
                      root       810     1  0 08:00 ?        00:00:00 /usr/sbin/sshd\n";
        let records = parse(input);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].pid, 1);
        assert_eq!(records[0].ppid, 0);
        assert_eq!(records[0].command, "/sbin/init");
        assert_eq!(records[1].pid, 810);
        assert_eq!(records[1].ppid, 1);
        assert_eq!(records[1].command, "/usr/sbin/sshd");
    }

    #[test]
    fn parse_ps_ef_header_is_case_insensitive_and_accepts_command_column() {
        let input = "uid pid ppid command\nroot 1 0 init\n";
        let records = parse(input);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].pid, 1);
        assert_eq!(records[0].command, "init");
    }

    #[test]
    fn format_renders_ps_ef_input_as_tree() {
        let input = "UID   PID  PPID CMD\n\
                      root    1     0 init\n\
                      root  810     1 sshd\n";
        let records = parse(input);
        assert_eq!(format(&records), "init (1)\n└── sshd (810)\n");
    }

    #[test]
    fn format_single_root_no_children() {
        let records = parse("1 0 init");
        assert_eq!(format(&records), "init (1)\n");
    }

    #[test]
    fn format_nested_tree_matches_readme_example() {
        let input = "810 1 sshd\n1 0 init\n2200 810 bash\n2350 2200 vim\n900 1 cron\n";
        let records = parse(input);
        let expected = "init (1)\n\
                         ├── sshd (810)\n\
                         │   └── bash (2200)\n\
                         │       └── vim (2350)\n\
                         └── cron (900)\n";
        assert_eq!(format(&records), expected);
    }

    #[test]
    fn format_sorts_roots_and_siblings_by_pid() {
        let input = "50 0 c\n10 0 a\n30 0 b\n";
        let records = parse(input);
        let expected = "a (10)\nb (30)\nc (50)\n";
        assert_eq!(format(&records), expected);
    }

    #[test]
    fn format_treats_missing_parent_as_root() {
        // ppid 999 was never given its own line, so this becomes a root.
        let records = parse("5 999 orphan");
        assert_eq!(format(&records), "orphan (5)\n");
    }

    #[test]
    fn format_treats_self_parent_as_root() {
        let records = parse("7 7 loopy");
        assert_eq!(format(&records), "loopy (7)\n");
    }

    #[test]
    fn format_keeps_first_of_duplicate_pid() {
        let input = "1 0 init\n1 0 impostor\n";
        let records = parse(input);
        assert_eq!(format(&records), "init (1)\n");
    }

    #[test]
    fn diagnose_reports_nothing_for_clean_input() {
        let records = parse("1 0 init\n2 1 sshd\n");
        let diag = diagnose(&records);
        assert!(diag.is_empty());
    }

    #[test]
    fn diagnose_reports_self_parented_pid() {
        let records = parse("7 7 loopy");
        let diag = diagnose(&records);
        assert_eq!(diag.self_parented, vec![7]);
        assert!(diag.missing_parent.is_empty());
        assert!(diag.cycles.is_empty());
    }

    #[test]
    fn diagnose_reports_missing_parent() {
        let records = parse("5 999 orphan");
        let diag = diagnose(&records);
        assert_eq!(diag.missing_parent, vec![(5, 999)]);
        assert!(diag.self_parented.is_empty());
        assert!(diag.cycles.is_empty());
    }

    #[test]
    fn diagnose_reports_mutual_cycle() {
        // 1 and 2 name each other as parent, so neither ever reaches a root.
        let records = parse("1 2 a\n2 1 b\n");
        let diag = diagnose(&records);
        assert_eq!(diag.cycles, vec![1, 2]);
        assert!(diag.self_parented.is_empty());
        assert!(diag.missing_parent.is_empty());
        // format() silently drops cycle members rather than looping forever.
        assert_eq!(format(&records), "");
    }

    #[test]
    fn diagnose_reports_cycle_with_a_tail_hanging_off_it() {
        // 3's ancestry runs 3 -> 2 -> 1 -> 2, looping through the 1/2 cycle,
        // so all three pids are unreachable from any root.
        let records = parse("1 2 a\n2 1 b\n3 2 c\n");
        let diag = diagnose(&records);
        assert_eq!(diag.cycles, vec![1, 2, 3]);
    }

    #[test]
    fn format_ascii_uses_plain_connectors() {
        let input = "810 1 sshd\n1 0 init\n2200 810 bash\n900 1 cron\n";
        let records = parse(input);
        let expected = "init (1)\n\
                         |-- sshd (810)\n\
                         |   `-- bash (2200)\n\
                         `-- cron (900)\n";
        assert_eq!(format_ascii(&records), expected);
    }
}
