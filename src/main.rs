use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

mod tree;

fn main() -> ExitCode {
    let mut ascii = false;
    let mut paths: Vec<String> = Vec::new();
    for arg in env::args().skip(1) {
        if arg == "--ascii" {
            ascii = true;
        } else {
            paths.push(arg);
        }
    }

    let input = match read_input(&paths) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pstidy: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let records = tree::parse(&input);
    if records.is_empty() {
        eprintln!("pstidy: no process records found in input");
        return ExitCode::FAILURE;
    }

    let output = if ascii {
        tree::format_ascii(&records)
    } else {
        tree::format(&records)
    };
    print!("{}", output);
    ExitCode::SUCCESS
}

/// With no file arguments, reads stdin so the tool can sit in a pipeline
/// (e.g. `ps -eo pid,ppid,comm | pstidy`). With arguments, treats each
/// one as a path and concatenates them in order.
fn read_input(paths: &[String]) -> io::Result<String> {
    if paths.is_empty() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }

    let mut buf = String::new();
    for path in paths {
        buf.push_str(&fs::read_to_string(path)?);
        if !buf.ends_with('\n') {
            buf.push('\n');
        }
    }
    Ok(buf)
}
