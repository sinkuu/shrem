#![feature(convert)]
#![feature(path_ext)]

extern crate clap;

use clap::{App, Arg};
use std::fmt;
use std::fs::PathExt;
use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Copy, Clone)]
struct Args {
    recursive: bool,
    force: bool,
    verbose: bool,
    interactive: bool,
    iterations: Option<usize>,
}

fn main() {
    let matches = App::new("shrem")
        .version("0.1.0")
        .about("Overwrite the specified FILE(s) repeatedly and then remove it")
        .arg(Arg::with_name("FILE")
             .multiple(true)
             .index(1))
        .arg(Arg::with_name("force")
             .short("f")
             .long("force")
             .help("Ignore nonexistent files and arguments"))
        .arg(Arg::with_name("recursive")
             .short("r")
             .long("recursive")
             .help("Remove directories and their contents recursively"))
        .arg(Arg::with_name("verbose")
             .short("v")
             .long("verbose")
             .help("Explain what is being done"))
        .arg(Arg::with_name("interactive")
             .short("i")
             .long("interactive")
             .help("Prompt before removal"))
        .arg(Arg::with_name("N")
             .short("n")
             .long("iterations")
             .help("Overwrite N times instead of default (3)")
             .takes_value(true))
        .get_matches();

    let args = Args {
        recursive: matches.is_present("recursive"),
        force: matches.is_present("force"),
        verbose: matches.is_present("verbose"),
        interactive: matches.is_present("interactive"),
        iterations: matches.value_of("N")
            .and_then(|s| s.parse::<usize>().ok()),
    };

    if args.recursive {
        for f in matches.values_of("FILE").unwrap_or(vec![]) {
            recursive_shred(f, &args).unwrap();
        }
    } else {
        let mut paths = matches.values_of("FILE").unwrap_or(vec![]);
        if args.force {
            paths = paths.into_iter()
                .filter(|ps| {
                    let p = Path::new(ps);
                    p.exists() && p.is_file()
                }).collect();
        }

        if paths.len() > 0 {
            let mut shred_cmd = get_shred_cmd(&args);
            shred_cmd.args(&paths);
            std::process::exit(shred_cmd.status().unwrap().code().unwrap());
        }
    }
}

fn recursive_shred<P: AsRef<Path>>(path: P, args: &Args) -> io::Result<()> {
    use std::fs;

    let path = path.as_ref();

    if args.force && !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        if !args.interactive ||
                try!(prompt(format_args!("descend into directory '{}'?", path.display()))) {
            for entry in try!(fs::read_dir(path)) {
                try!(recursive_shred(try!(entry).path(), args));
            }
            try!(shred_dir(path, args));
        }
    } else {
        if !args.interactive || try!(prompt(format_args!("remove file '{}'?", path.display()))) {
            let mut shred_cmd = get_shred_cmd(args);
            shred_cmd.arg(path.as_os_str());
            let status = try!(shred_cmd.status());
            if !status.success() {
                std::process::exit(status.code().unwrap());
            }
        }
    }

    Ok(())
}

fn shred_dir<P: AsRef<Path>>(path: P, args: &Args) -> io::Result<()> {
    use std::fs;
    use std::iter;

    let mut path = path.as_ref().to_path_buf();
    if !args.interactive || try!(prompt(format_args!("remove directory '{}'?", path.display()))) {
        if let Some(name) = path.clone().file_name() {
            let len = name.to_bytes().unwrap().len();
            for n in (1..len+1).rev() {
                let mut s = String::new();
                s.extend(iter::repeat('0').take(n));

                let new_path = path.with_file_name(&s);
                try!(fs::rename(&path, &new_path));
                if args.verbose { println!("shrem: {}: renamed to {}", path.display(), new_path.display()); }
                path = new_path;
            }
        }

        if args.verbose { println!("shrem: {}: removing", path.display()); }
        try!(fs::remove_dir(&path));
    }

    Ok(())
}

fn get_shred_cmd(args: &Args) -> Command {
    let mut shred_cmd = Command::new("shred");
    shred_cmd.args(&["-z", "-u"][..]);
    if args.verbose {
        shred_cmd.arg("-v");
    }
    if let Some(n) = args.iterations {
        shred_cmd.arg(fmt::format(format_args!("{}", n)));
    }
    shred_cmd
}

fn prompt(args: fmt::Arguments) -> io::Result<bool> {
    use std::io::Write;

    print!("{} ", args);
    try!(io::stdout().flush());
    let mut s = String::new();
    try!(io::stdin().read_line(&mut s));
    match s.chars().next() {
        Some('y') | Some('Y') => Ok(true),
        _ => Ok(false),
    }
}
