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

/// Renders records as a tree, ├──/└── style, sorted by pid at every
/// level so the same input always produces the same output.
pub fn format(records: &[Record]) -> String {
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
        write_node(*root, "", is_last, true, &by_pid, &children, &mut out);
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
    out: &mut String,
) {
    let record = match by_pid.get(&pid) {
        Some(r) => r,
        None => return,
    };

    if !is_root {
        out.push_str(prefix);
        out.push_str(if is_last { "└── " } else { "├── " });
    }
    out.push_str(&record.command);
    out.push_str(" (");
    out.push_str(&record.pid.to_string());
    out.push_str(")\n");

    let child_prefix = if is_root {
        String::new()
    } else {
        format!("{}{}", prefix, if is_last { "    " } else { "│   " })
    };

    if let Some(kids) = children.get(&pid) {
        for (i, kid) in kids.iter().enumerate() {
            let last = i + 1 == kids.len();
            write_node(*kid, &child_prefix, last, false, by_pid, children, out);
        }
    }
}
