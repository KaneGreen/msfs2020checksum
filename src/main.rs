use clap::Parser;
use dirs::{data_dir, data_local_dir};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Result as IoResult, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use walkdir::WalkDir;
use xxhash_rust::xxh3;

/// xxhash checksum for MSFS 2020 data files
#[derive(Parser, Debug)]
#[clap(version, about)]
struct Args {
    /// Force use this path as the `UserCfg.opt` file
    #[clap(short = 'c', long = "config")]
    cfgfile: Option<PathBuf>,

    /// Force use this path as the `InstalledPackagesPath` directory
    /// (Setting this argument will ignore the `config` argument)
    #[clap(short = 'P', long = "packages")]
    packages: Option<PathBuf>,

    /// The number of multi-threaded parallelism
    /// (0 means the number of CPU threads)
    #[clap(short = 'T', long, default_value_t = 0)]
    threads: usize,

    /// Path for the output file
    /// (If the file already exists it will be overwritten)
    #[clap(short = 'o', long)]
    output: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    let packages_dir = match args.packages {
        Some(packages_path) => packages_path,
        None => {
            let usercfg = match args.cfgfile {
                Some(cfg_path) => cfg_path,
                None => match find_msfs_usercfg() {
                    Some(cfg_path) => cfg_path,
                    None => {
                        panic!("Unable to find the `UserCfg.opt` file, you may not have correctly installed MSFS2020.");
                    }
                },
            };
            match get_msfs_packages_dir(&usercfg) {
                Some(mut packages_path) => {
                    packages_path.push("Official");
                    packages_path
                }
                None => {
                    panic!("Unable to find the `InstalledPackagesPath` configuration in the `UserCfg.opt` file.");
                }
            }
        }
    };
    eprintln!(
        "Using MSFS 2020 InstalledPackagesPath: {:?}\n",
        packages_dir.to_string_lossy()
    );

    match args.output {
        Some(ref outpath) => {
            if outpath.exists() {
                let meta = outpath.metadata().unwrap();
                if meta.is_dir() {
                    panic!("Output path is a directory: {:?}", outpath);
                } else {
                    eprintln!("Warning: output file will be overwritten: {:?}\n", outpath);
                }
            }
        }
        None => {
            eprintln!("The hash values will be output to the stdout.\n");
        }
    }

    let thread_num = if args.threads == 0 {
        thread::available_parallelism().unwrap().into()
    } else {
        args.threads
    };

    let print_screen = args.output.is_none();
    let mut results = Vec::new();

    if packages_dir.metadata().unwrap().is_dir() {
        let s_package_files = Mutex::new(
            WalkDir::new(&packages_dir)
                .follow_links(true) // Do we really need to follow the link?
                .into_iter()
                .filter_map(|res| res.ok()),
        );
        let buffersize = get_buffer_size(thread_num) as usize;
        eprintln!(
            "Threads: {}\nMemory buffer: {} MiB per thread.\n",
            thread_num,
            buffersize / 1024 / 1024
        );
        thread::scope(|s| {
            let mut t_handles = Vec::new();
            for _ in 0..thread_num {
                let thread_package_files = &s_package_files;
                let thread_packages_dir = &packages_dir;
                let handle = s.spawn(move || {
                    let mut result = Vec::new();
                    let mut buffer = vec![0xFF; buffersize];
                    loop {
                        let package_file;
                        {
                            let mut file_iter = thread_package_files.lock().unwrap();
                            package_file = match file_iter.next() {
                                Some(entry) => entry.into_path(),
                                None => break,
                            };
                        }
                        match get_xxhash3_128_and_size(&package_file, &mut buffer[..]) {
                            Ok(Some((hash, filesize))) => {
                                let relative_path =
                                    match package_file.strip_prefix(&thread_packages_dir) {
                                        Ok(r_path) => r_path.to_path_buf(),
                                        Err(_) => package_file,
                                    };
                                let path_string = relative_path.to_string_lossy().to_string();
                                if print_screen {
                                    println!("{:032x}\t{:10}\t{}", hash, filesize, path_string)
                                }
                                result.push((path_string, hash, filesize));
                            }
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!(
                                    "Fail to read file {} {}",
                                    package_file.to_string_lossy(),
                                    err
                                );
                            }
                        }
                    }
                    result.sort_unstable();
                    result
                });
                t_handles.push(handle);
            }
            for handle in t_handles {
                let mut result = handle.join().unwrap();
                results.append(&mut result);
            }
        });
        results.sort_unstable();
    } else {
        eprintln!(
            "{} is a file. Processing with single-threaded.\n",
            packages_dir.to_string_lossy()
        );
        let buffersize = get_buffer_size(1) as usize;
        eprintln!("Memory buffer: {} MiB.\n", buffersize / 1024 / 1024);
        let mut buffer = vec![0xFF; buffersize];
        match get_xxhash3_128_and_size(&packages_dir, &mut buffer[..]) {
            Ok(Some((hash, filesize))) => {
                let path_string = packages_dir.to_string_lossy().to_string();
                if print_screen {
                    println!("{:032x}\t{:10}\t{}", hash, filesize, path_string)
                }
                results.push((path_string, hash, filesize));
            }
            Ok(_) => {
                unreachable!();
            }
            Err(err) => {
                eprintln!(
                    "Fail to read file {} {}",
                    packages_dir.to_string_lossy(),
                    err
                );
            }
        }
    }
    if let Some(outpath) = args.output {
        let fhw = File::create(outpath).unwrap();
        let mut writer = BufWriter::new(fhw);
        for (path, hash, filesize) in results {
            writer
                .write_fmt(format_args!("{:032x}\t{:10}\t{}\r\n", hash, filesize, path))
                .unwrap();
        }
        writer.flush().unwrap();
    }
}

fn find_msfs_usercfg() -> Option<PathBuf> {
    const STORE_MSFS_DIR_NAME: &str = "Microsoft.FlightSimulator_8wekyb3d8bbwe";
    const STEAM_MSFS_DIR_NAME: &str = "Microsoft Flight Simulator";
    {
        let mut store_cfg = data_local_dir().unwrap();
        store_cfg.push("Packages");
        store_cfg.push(STORE_MSFS_DIR_NAME);
        store_cfg.push("LocalCache");
        store_cfg.push("UserCfg.opt");
        if store_cfg.is_file() {
            return Some(store_cfg);
        }
    }
    {
        let mut steam_cfg = data_dir().unwrap();
        steam_cfg.push(STEAM_MSFS_DIR_NAME);
        steam_cfg.push("UserCfg.opt");
        if steam_cfg.is_file() {
            return Some(steam_cfg);
        }
    }
    {
        for entry in WalkDir::new(data_dir().unwrap())
            .follow_links(true)
            .into_iter()
            .filter_map(|res| res.ok())
        {
            if entry.file_type().is_file() && entry.file_name() == "UserCfg.opt" {
                let path = entry.path().to_str().unwrap();
                let path = path.to_ascii_lowercase();
                if path.contains("microsoft") && path.contains("flight") {
                    return Some(entry.path().to_path_buf());
                }
            }
        }
    }
    None
}

fn get_msfs_packages_dir(usercfg: &Path) -> Option<PathBuf> {
    let fhr = File::open(usercfg).unwrap();
    for line in BufReader::new(fhr).lines() {
        let line = line.unwrap();
        let line = line.trim();
        if line.starts_with("InstalledPackagesPath") {
            let path_txt = line.split_once(' ').unwrap().1;
            return Some(PathBuf::from(&path_txt[1..(path_txt.len() - 1)]));
        }
    }
    None
}

fn get_xxhash3_128_and_size(file: &Path, buffer: &mut [u8]) -> IoResult<Option<(u128, u64)>> {
    let meta = file.metadata()?;
    if meta.is_dir() {
        return Ok(None);
    }
    let filesize = meta.len();
    let hash = if filesize > buffer.len() as u64 {
        bigfile_xxhash3_128(file, buffer)
    } else {
        smallfile_xxhash3_128(file, buffer)
    };
    Ok(Some((hash, filesize)))
}

fn bigfile_xxhash3_128(file: &Path, buffer: &mut [u8]) -> u128 {
    let mut fhr = File::open(file).unwrap();
    let mut hasher = xxh3::Xxh3::new();
    loop {
        let read_size = fhr.read(buffer).unwrap();
        if read_size != 0 {
            hasher.update(&buffer[..read_size]);
        } else {
            break;
        }
    }
    hasher.digest128()
}

fn smallfile_xxhash3_128(file: &Path, buffer: &mut [u8]) -> u128 {
    let mut fhr = File::open(file).unwrap();
    let read_size = fhr.read(buffer).unwrap();
    xxh3::xxh3_128(&buffer[..read_size])
}

fn get_buffer_size(thread_number: usize) -> usize {
    // A large buffer can take advantage of the sequential read performance of the
    // hard disk as much as possible, whether it is a mechanical hard disk or a
    // solid-state disk.
    // But this will significantly increase the memory usage. For example, in a
    // 16-thread scenario, this would consume 4GB of memory.
    // However, I believe flight sim users should have 16GB+ of memory.
    const DEFAULT_BUFFERIZE: u64 = 256 * 1024 * 1024;
    const MINIMAL_BUFFERIZE: u64 = 16 * 1024 * 1024;
    let available_memory_all = available_memory();
    let available_memory_per_thread = available_memory_all / thread_number as u64;
    let mut bufferize = DEFAULT_BUFFERIZE;
    while bufferize > available_memory_per_thread && bufferize >= MINIMAL_BUFFERIZE {
        bufferize /= 2;
    }
    if bufferize > available_memory_per_thread {
        panic!(
            "No enough memory: current {:.3} MiB",
            available_memory_all as f64 / 1024.0 / 1024.0
        );
    }
    bufferize as usize
}

#[cfg(target_os = "windows")]
fn available_memory() -> u64 {
    use std::mem::{size_of, zeroed};
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    unsafe {
        let mut mem_info: MEMORYSTATUSEX = zeroed();
        mem_info.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        GlobalMemoryStatusEx(&mut mem_info).unwrap();
        mem_info.ullAvailPhys
    }
}
