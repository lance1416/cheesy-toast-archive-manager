use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};

use cheesy_toast_archive_manager_lib::archive::get_backend;
use cheesy_toast_archive_manager_lib::archive::writer::{
    collect_entries, ArchiveWriter, WriteOptions,
};
use cheesy_toast_archive_manager_lib::archive::writer_libarchive::{
    TarArchiveWriter, TarCompression,
};
use cheesy_toast_archive_manager_lib::archive::writer_sevenz::SevenZArchiveWriter;
use cheesy_toast_archive_manager_lib::archive::writer_zip::ZipArchiveWriter;

#[derive(Parser)]
#[command(name = "ct", about = "Cheesy Toast archive manager", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the contents of an archive
    List {
        archive: PathBuf,
        /// Force filename encoding (e.g. GBK, Shift_JIS)
        #[arg(short, long)]
        encoding: Option<String>,
        /// Password for encrypted archives
        #[arg(short, long)]
        password: Option<String>,
        /// Encoding used to encode the password bytes (legacy archives)
        #[arg(long)]
        password_encoding: Option<String>,
    },
    /// Extract files from an archive
    Extract {
        archive: PathBuf,
        /// Specific archive paths to extract (default: all files)
        entries: Vec<String>,
        /// Destination directory
        #[arg(short, long, default_value = ".")]
        dest: PathBuf,
        /// Force filename encoding (e.g. GBK, Shift_JIS)
        #[arg(short, long)]
        encoding: Option<String>,
        /// Password for encrypted archives
        #[arg(short, long)]
        password: Option<String>,
        /// Encoding used to encode the password bytes (legacy archives)
        #[arg(long)]
        password_encoding: Option<String>,
    },
    /// Create a new archive from files and directories
    Create {
        /// Destination archive path (format inferred from extension)
        dest: PathBuf,
        /// Source files and directories to include
        sources: Vec<PathBuf>,
        /// Archive format override: zip, 7z, tar, tar.gz, tar.bz2, tar.xz
        #[arg(short, long)]
        format: Option<String>,
        /// Compression level (0–9)
        #[arg(short, long)]
        level: Option<u8>,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli.command) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::List {
            archive,
            encoding,
            password,
            password_encoding,
        } => cmd_list(
            &archive,
            encoding.as_deref(),
            password.as_deref(),
            password_encoding.as_deref(),
        ),

        Command::Extract {
            archive,
            entries,
            dest,
            encoding,
            password,
            password_encoding,
        } => cmd_extract(
            &archive,
            &entries,
            &dest,
            encoding.as_deref(),
            password.as_deref(),
            password_encoding.as_deref(),
        ),

        Command::Create {
            dest,
            sources,
            format,
            level,
        } => cmd_create(&dest, &sources, format.as_deref(), level),
    }
}

fn cmd_list(
    archive: &Path,
    encoding: Option<&str>,
    password: Option<&str>,
    password_encoding: Option<&str>,
) -> Result<(), String> {
    let (backend, paths) = get_backend(archive).map_err(|e| e.to_string())?;
    let vfs = backend
        .parse_upfront(&paths, encoding, password, password_encoding)
        .map_err(|e| e.to_string())?;

    for node in &vfs.entries {
        let kind = if node.is_dir { 'd' } else { 'f' };
        println!(
            "{kind}  {:>10}  [{:^8}]  {}",
            node.size, node.encoding_used, node.path
        );
    }

    println!("\n{} entries", vfs.total_entries);
    Ok(())
}

fn cmd_extract(
    archive: &Path,
    entries: &[String],
    dest: &Path,
    encoding: Option<&str>,
    password: Option<&str>,
    password_encoding: Option<&str>,
) -> Result<(), String> {
    let (backend, paths) = get_backend(archive).map_err(|e| e.to_string())?;
    let vfs = backend
        .parse_upfront(&paths, encoding, password, password_encoding)
        .map_err(|e| e.to_string())?;

    let to_extract: Vec<_> = vfs
        .entries
        .iter()
        .filter(|n| {
            !n.is_dir
                && (entries.is_empty() || entries.iter().any(|e| e == &n.path || e == &n.name))
        })
        .collect();

    if to_extract.is_empty() {
        return Err("no matching entries found".to_string());
    }

    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    let mut count = 0;
    for node in &to_extract {
        let out_path = dest.join(&node.path);
        backend
            .extract_node(&paths, node, &out_path, password, password_encoding)
            .map_err(|e| format!("{}: {}", node.path, e))?;
        println!("  extracted  {}", node.path);
        count += 1;
    }

    println!(
        "{count} file{} extracted to {}",
        if count == 1 { "" } else { "s" },
        dest.display()
    );
    Ok(())
}

fn cmd_create(
    dest: &Path,
    sources: &[PathBuf],
    format_override: Option<&str>,
    level: Option<u8>,
) -> Result<(), String> {
    let format = resolve_format(dest, format_override)?;

    let entries = collect_entries(sources).map_err(|e| e.to_string())?;
    if entries.is_empty() {
        return Err("no source files found".to_string());
    }

    let options = WriteOptions {
        compression_level: level,
    };

    let writer: Box<dyn ArchiveWriter + Send + Sync> = match format.as_str() {
        "zip" => Box::new(ZipArchiveWriter),
        "7z" => Box::new(SevenZArchiveWriter),
        "tar" => Box::new(TarArchiveWriter {
            compression: TarCompression::None,
        }),
        "tar.gz" | "tgz" => Box::new(TarArchiveWriter {
            compression: TarCompression::Gzip,
        }),
        "tar.bz2" => Box::new(TarArchiveWriter {
            compression: TarCompression::Bzip2,
        }),
        "tar.xz" => Box::new(TarArchiveWriter {
            compression: TarCompression::Xz,
        }),
        other => return Err(format!("unsupported format '{other}'")),
    };

    println!(
        "creating {} ({} file{})…",
        dest.display(),
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );
    writer
        .create(&entries, dest, &options)
        .map_err(|e| e.to_string())?;

    let size = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    println!("done — {} bytes", size);
    Ok(())
}

fn resolve_format(dest: &Path, override_fmt: Option<&str>) -> Result<String, String> {
    if let Some(fmt) = override_fmt {
        return Ok(fmt.to_string());
    }
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Ok("tar.gz".to_string());
    }
    if name.ends_with(".tar.bz2") {
        return Ok("tar.bz2".to_string());
    }
    if name.ends_with(".tar.xz") {
        return Ok("tar.xz".to_string());
    }
    if name.ends_with(".tar") {
        return Ok("tar".to_string());
    }

    match dest.extension().and_then(|e| e.to_str()) {
        Some("zip") => Ok("zip".to_string()),
        Some("7z") => Ok("7z".to_string()),
        Some(ext) => Err(format!("cannot infer format from '.{ext}'; use --format")),
        None => Err("no file extension on destination; use --format".to_string()),
    }
}
