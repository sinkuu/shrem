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
            .help("Ignore nonexistent files and errors"))
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
        if let Some(iter) = matches.values_of("FILE") {
            let mut err = false;

            for p in iter {
                let path = Path::new(p);
                if path.is_dir() {
                    if let Err(e) = shred_dir(&path, &config) {
                        writeln!(io::stderr(),
                                 "shrem: cannot remove directory '{}': {}",
                                 path.display(),
                                 e)
                            .unwrap();
                        err = true;
                    }
                } else {
                    if let Err(e) = shred_file(&path, &config) {
                        writeln!(io::stderr(),
                                 "shrem: cannot remove file '{}': {}",
                                 path.display(),
                                 e)
                            .unwrap();
                        err = true;
                    }
                }

                if err && !config.force {
                    std::process::exit(1);
                }
            }

            if err {
                std::process::exit(1);
            }
        }
    } else {
        if let Some(paths) = matches.values_of("FILE") {
            let paths = paths.map(|iter| iter.map(PathBuf::from));

            let mut err = false;
            for p in paths {
                if let Err(e) = shred_file(&p, &config) {
                    err = true;

                    writeln!(io::stderr(),
                             "shrem: cannot remove '{}': {}",
                             p.display(),
                             e)
                        .unwrap();

                    if !config.force {
                        std::process::exit(1);
                    }
                }
            }

            if err {
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug)]
enum ShremError {
    IoError(io::Error),
    PreservedRootError,
    ExternalProcessError(ExitStatus),
    NotFound(PathBuf),
    IsADirectory(PathBuf),
}

impl Display for ShremError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            ShremError::IoError(ref e) => e.fmt(f),
            ref other => f.write_str(Error::description(other)),
        }
    }
}

impl Error for ShremError {
    fn description(&self) -> &str {
        match *self {
            ShremError::IoError(ref e) => e.description(),
            ShremError::PreservedRootError => {
                "It is dangerous to operate on '/' recursively. \
                 Use --no-preserve-root to override this failsafe."
            }
            ShremError::ExternalProcessError(_) => "External process exited with an error.",
            ShremError::NotFound(_) => "No such file or directory",
            ShremError::IsADirectory(_) => "Is a directory",
        }
    }

    fn cause(&self) -> Option<&Error> {
        if let ShremError::IoError(ref e) = *self {
            Some(e)
        } else {
            None
        }
    }
}

impl From<io::Error> for ShremError {
    fn from(e: io::Error) -> ShremError {
        ShremError::IoError(e)
    }
}

fn shred_file<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), ShremError> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(ShremError::NotFound(path.to_path_buf()));
    }

    if path.is_dir() {
        return Err(ShremError::IsADirectory(path.to_path_buf()));
    }

    if !config.interactive || prompt(format_args!("remove file '{}'?", path.display()))? {
        let mut shred_cmd = get_shred_cmd(config);
        shred_cmd.arg(path.as_os_str());
        let status = shred_cmd.status()?;
        if !status.success() {
            return Err(ShremError::ExternalProcessError(status));
        }
    }

    Ok(())
}

fn shred_dir<P: AsRef<Path>>(path: P, config: &Config) -> Result<(), ShremError> {
    use std::os::unix::ffi::OsStrExt;

    let mut path = path.as_ref().to_path_buf();

    if !path.exists() {
        return Err(ShremError::NotFound(path.to_path_buf()));
    }

    assert!(path.is_dir());

    if config.preserve_root && path.is_absolute() && path.parent().is_none() {
        return Err(ShremError::PreservedRootError);
    }

    if config.no_remove {
        return Ok(());
    }

    if !config.interactive || prompt(format_args!("remove directory '{}'?", path.display()))? {
        if config.verbose {
            println!("shrem: {}: removing", path.display());
        }

        if let Some(len) = path.file_name().map(|name| name.as_bytes().len()) {
            for n in (1..len + 1).rev() {
                let new_path = match generate_new_path(&path, n) {
                    None => break,
                    Some(p) => p,
                };

                if config.verbose {
                    println!("shrem: {}: renamed to {}",
                             path.display(),
                             new_path.display());
                }
                fs::rename(&path, &new_path)?;
                path = new_path;
            }
        }

        fs::remove_dir(&path)?;
        if config.verbose {
            println!("shrem: {}: removed", path.display());
        }
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
        shred_cmd.arg(&format!("-n {}", n));
    }
    shred_cmd
}

fn prompt(config: fmt::Arguments) -> io::Result<bool> {
    print!("{} ", config);
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    match s.chars().next() {
        Some('y') | Some('Y') => Ok(true),
        _ => Ok(false),
    }
}

fn generate_new_path<P: AsRef<Path>>(path: P, length: usize) -> Option<PathBuf> {
    let mut path = path.as_ref().to_path_buf();

    static CHARS: &'static [u8] =
        b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";

    let mut idxs = vec![0; length];
    let mut s = String::with_capacity(length);
    while idxs[0] < CHARS.len() {
        s.clear();
        s.extend(idxs.iter().map(|&i| char::from(CHARS[i])));
        path.set_file_name(&s);
        if !path.exists() {
            return Some(path);
        }

        for (i, e) in idxs.iter_mut().enumerate().rev() {
            *e += 1;
            if i != 0 && *e == CHARS.len() {
                *e = 0;
            } else {
                break;
            }
        }
    }

    None
}
