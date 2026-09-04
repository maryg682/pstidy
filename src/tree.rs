use std::collections::{HashMap, HashSet};

pub struct Record {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
}

/// Parses "pid ppid command..." lines, tolerating extra whitespace,
/// blank lines, and '#' comments. Lines that don't have at least a
/// numeric pid and ppid are skipped rather than treated as errors,
/// since real ps/pstree dumps often carry a header row or stray notes.
pub fn parse(input: &str) -> Vec<Record> {
    let mut records = Vec::new();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut fields = line.split_whitespace();
        let pid = match fields.next().and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let ppid = match fields.next().and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };

        let command: String = fields.collect::<Vec<_>>().join(" ");
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

fn format_with(records: &[Record], connectors: &Connectors) -> String {
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
        // A record is a root if its parent is missing from the input
        // (pid 0, or a ppid we were never given a line for).
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

    let by_pid: HashMap<u32, &Record> = unique.iter().map(|r| (r.pid, *r)).collect();

    let mut out = String::new();
    for (i, root) in roots.iter().enumerate() {
        let is_last = i + 1 == roots.len();
        write_node(*root, "", is_last, true, &by_pid, &children, connectors, &mut out);
    }
    out
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
