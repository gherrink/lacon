//! Deterministic stdout+stderr emitter used by Phase 1 integration tests.
//! Phase 5 (bundled rules) will reuse this for fixture-driven tests.

use clap::Parser;
use std::io::Write;

#[derive(Parser, Debug)]
#[command(version, about = "Deterministic stdout+stderr emitter for tests")]
struct Args {
    #[arg(long, default_value_t = 0)]
    stdout_lines: usize,
    #[arg(long, default_value_t = 0)]
    stderr_lines: usize,
    #[arg(long, default_value_t = 0)]
    mix: usize,
    #[arg(long)]
    ansi: bool,
    #[arg(long, default_value_t = 0)]
    errors: usize,
    #[arg(long, default_value_t = 0)]
    exit: i32,
    #[arg(long, default_value_t = 0)]
    bytes: usize,
}

fn ansi_wrap(line: &str, with_ansi: bool) -> String {
    if with_ansi {
        format!("\x1b[31m{}\x1b[0m", line)
    } else {
        line.to_owned()
    }
}

fn main() {
    let args = Args::parse();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut so = stdout.lock();
    let mut se = stderr.lock();

    for i in 1..=args.stdout_lines {
        let line = format!("line {}", i);
        writeln!(so, "{}", ansi_wrap(&line, args.ansi)).unwrap();
    }
    for i in 1..=args.stderr_lines {
        let line = format!("err {}", i);
        writeln!(se, "{}", ansi_wrap(&line, args.ansi)).unwrap();
    }
    for i in 1..=args.mix {
        let line = format!("mix {}", i);
        if i % 2 == 0 {
            writeln!(se, "{}", ansi_wrap(&line, args.ansi)).unwrap();
        } else {
            writeln!(so, "{}", ansi_wrap(&line, args.ansi)).unwrap();
        }
    }
    for i in 1..=args.errors {
        writeln!(so, "FAIL error {}", i).unwrap();
    }
    if args.bytes > 0 {
        // Emit N bytes of `a` followed by a newline.
        let mut written = 0;
        while written < args.bytes {
            let chunk = (args.bytes - written).min(80);
            let s: String = std::iter::repeat('a').take(chunk).collect();
            writeln!(so, "{}", s).unwrap();
            written += chunk + 1; // +1 for \n
        }
    }
    so.flush().unwrap();
    se.flush().unwrap();
    std::process::exit(args.exit);
}
