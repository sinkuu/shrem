extern crate clap;

use clap::{App, Arg};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fmt;
use std::fs;
use std::io::Write;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::result::Result;

#[derive(Debug, Copy, Clone)]
struct Config {
    recursive: bool,
    force: bool,
    verbose: bool,
    interactive: bool,
    preserve_root: bool,
    no_remove: bool,
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
        .arg(Arg::with_name("preserve-root")
             .long("preserve-root")
             .conflicts_with("no-preserve-root")
             .help("Do not remove '/' (default)"))
        .arg(Arg::with_name("no-preserve-root")
             .long("no-preserve-root")
             .conflicts_with("preserve-root")
             .help("Allow removing '/'"))
        .arg(Arg::with_name("no-remove")
             .long("no-remove")
             .help("Don't remove files (only overwrite)"))
        .arg(Arg::with_name("N")
             .short("n")
             .long("iterations")
             .help("Overwrite N times instead of default (3)")
             .takes_value(true))
        .get_matches();

    let config = Config {
        recursive: matches.is_present("recursive"),
        force: matches.is_present("force"),
        verbose: matches.is_present("verbose"),
        interactive: matches.is_present("interactive"),
        preserve_root: matches.is_present("preserve-root") ||
            !matches.is_present("no-preserve-root"),
        no_remove: matches.is_present("no-remove"),
        iterations: matches.value_of("N")
            .and_then(|s| s.parse::<usize>().ok()),
    };

    if config.recursive {
        let mut err = false;
        for f in matches.values_of("FILE").unwrap_or_default() {
            match recursive_shred(f, &config) {
                Ok(()) => (),
                Err(e) => {
                    err = true;
                    match e {
                        RecursiveShredError::ExternalProcessError(_) => (),
                        _ => writeln!(io::stderr(), "shrem: {}", e).unwrap(),
                    }
                }
            }
        }
        if err {
            std::process::exit(1);
        }
    } else {
        let mut paths = matches.values_of("FILE").unwrap_or_default();
        if config.force {
            paths = paths.into_iter()
                .filter(|ps| {
                    let p = Path::new(ps);
                    p.exists() && p.is_file()
                }).collect();
        }

        if !paths.is_empty() {
            let mut shred_cmd = get_shred_cmd(&config);
            shred_cmd.args(&paths);
            std::process::exit(shred_cmd.status().unwrap().code().unwrap());
        }
    }
}

#[derive(Debug)]
enum RecursiveShredError {
    IoError(io::Error),
    PreservedRootError,
    ExternalProcessError(ExitStatus),
}

impl Display for RecursiveShredError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let RecursiveShredError::IoError(ref e) = *self {
            e.fmt(f)
        } else {
            f.write_str(<Self as Error>::description(self))
        }
    }
}

impl Error for RecursiveShredError {
    fn description(&self) -> &str {
        match *self {
            RecursiveShredError::IoError(ref e) =>
                e.description(),
            RecursiveShredError::PreservedRootError =>
                "It is dangerous to operate on '/' recursively. Use --no-preserve-root to override this failsafe.",
            RecursiveShredError::ExternalProcessError(_) =>
                "external process exited with an error",
        }
    }

    fn cause(&self) -> Option<&Error> {
        if let RecursiveShredError::IoError(ref e) = *self {
            Some(e)
        } else {
            None
        }
    }
}

impl From<io::Error> for RecursiveShredError {
    fn from(e: io::Error) -> RecursiveShredError {
        RecursiveShredError::IoError(e)
    }
}

fn recursive_shred<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), RecursiveShredError> {
    let path = path.as_ref();

    if config.force && !path.exists() {
        return Ok(());
    }

    if config.preserve_root && path.is_absolute() && path.parent().is_none() {
        return Err(RecursiveShredError::PreservedRootError);
    }

    if path.is_dir() {
        if !config.interactive ||
                try!(prompt(format_args!("descend into directory '{}'?", path.display()))) {
            for entry in try!(fs::read_dir(path)) {
                try!(recursive_shred(try!(entry).path(), config));
            }
            if !config.no_remove {
                try!(shred_dir(path, config));
            }
        }
    } else {
        if !config.interactive || try!(prompt(format_args!("remove file '{}'?", path.display()))) {
            let mut shred_cmd = get_shred_cmd(config);
            shred_cmd.arg(path.as_os_str());
            let status = try!(shred_cmd.status());
            if !status.success() {
                return Err(RecursiveShredError::ExternalProcessError(status));
            }
        }
    }

    Ok(())
}

fn shred_dir<P: AsRef<Path>>(path: P, config: &Config) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let mut path = path.as_ref().to_path_buf();
    if !config.interactive || try!(prompt(format_args!("remove directory '{}'?", path.display()))) {
        if config.verbose { println!("shrem: {}: removing", path.display()); }

        if let Some(name) = path.clone().file_name() {
            let len = name.as_bytes().len();
            for n in (1..len+1).rev() {
                let new_path = match generate_new_path(&path, n) {
                    None => break,
                    Some(p) => p,
                };

                if config.verbose {
                    println!("shrem: {}: renamed to {}", path.display(), new_path.display());
                }
                try!(fs::rename(&path, &new_path));
                path = new_path;
            }
        }

        try!(fs::remove_dir(&path));
        if config.verbose { println!("shrem: {}: removed", path.display()); }
    }

    Ok(())
}

fn get_shred_cmd(config: &Config) -> Command {
    let mut shred_cmd = Command::new("shred");
    shred_cmd.arg("-z");
    if !config.no_remove {
        shred_cmd.arg("-u");
    }
    if config.verbose {
        shred_cmd.arg("-v");
    }
    if let Some(n) = config.iterations {
        shred_cmd.arg(fmt::format(format_args!("-n {}", n)));
    }
    shred_cmd
}

fn prompt(config: fmt::Arguments) -> io::Result<bool> {
    print!("{} ", config);
    try!(io::stdout().flush());
    let mut s = String::new();
    try!(io::stdin().read_line(&mut s));
    match s.chars().next() {
        Some('y') | Some('Y') => Ok(true),
        _ => Ok(false),
    }
}

fn generate_new_path<P: AsRef<Path>>(path: P, length: usize) -> Option<PathBuf> {
    let mut path = path.as_ref().to_path_buf();

    let chars: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_"
        .chars().collect();
    let mut idxs = vec![0; length];
    let mut s = String::with_capacity(length);
    while idxs[0] < chars.len() {
        s.clear();
        s.extend(idxs.iter().map(|&i| unsafe { *chars.get_unchecked(i) }));
        path.set_file_name(&s);
        if !path.exists() {
            return Some(path);
        }

        for (i, e) in idxs.iter_mut().enumerate().rev() {
            *e += 1;
            if i != 0 && *e == chars.len() {
                *e = 0;
            } else {
                break;
            }
        }
    }

    None
}
